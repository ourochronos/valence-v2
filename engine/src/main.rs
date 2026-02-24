//! Valence Engine Binary Entrypoint
//!
//! HTTP server exposing the triple store API.

use anyhow::Result;
#[cfg(any(feature = "postgres", feature = "embedded"))]
use anyhow::Context;
use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::signal;
use tracing::{info, warn};

use valence_engine::{api::create_router_with_store_type, ValenceEngine, EngineConfig};

/// Print startup banner with configuration summary
fn print_startup_banner(config: &EngineConfig) {
    let version = env!("CARGO_PKG_VERSION");
    let banner = r#"
╔══════════════════════════════════════════════════════════════════════════╗
║                          VALENCE ENGINE v{version}                          ║
║          Triple-based Knowledge Substrate with Topology Embeddings       ║
╚══════════════════════════════════════════════════════════════════════════╝
"#;
    
    println!("{}", banner.replace("{version}", version));
    info!("═══════════════════════════════════════════════════════════════════");
    info!("Configuration Summary:");
    info!("  Mode:         {}", config.server.mode);
    if config.server.mode == "http" || config.server.mode == "both" {
        info!("  Host:         {}", config.server.host);
        info!("  Port:         {}", config.server.port);
    }
    
    use valence_engine::config::StorageConfig;
    match &config.storage {
        #[cfg(feature = "embedded")]
        StorageConfig::Embedded { path, recompute_embeddings_on_start, embedding_dimensions } => {
            info!("  Database:     Sled (embedded, persistent)");
            info!("  Data path:    {}", path);
            info!("  Auto-rehydrate embeddings: {} ({}d)", recompute_embeddings_on_start, embedding_dimensions);
        }
        #[cfg(feature = "postgres")]
        StorageConfig::Postgres { url } => {
            info!("  Database:     PostgreSQL ({})", mask_password(url));
        }
        #[cfg(feature = "postgres")]
        StorageConfig::Tiered { database_url, hot_capacity, .. } => {
            info!("  Database:     Tiered (hot: {} triples, cold: PostgreSQL)", hot_capacity);
            info!("                PostgreSQL ({})", mask_password(database_url));
        }
        StorageConfig::Memory => {
            info!("  Database:     Memory (volatile)");
        }
    }
    
    info!("  Log Level:    {}", std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()));
    info!("═══════════════════════════════════════════════════════════════════");
}

/// Print engine status after initialization
async fn print_engine_status(engine: &ValenceEngine, store_type: &str) -> Result<()> {
    let triple_count = engine.store.count_triples().await?;
    let node_count = engine.store.count_nodes().await?;
    let lifecycle_status = engine.lifecycle_status().await?;
    let has_embeddings = engine.has_embeddings().await;
    
    info!("Engine Status:");
    info!("  Store Type:            {}", store_type);
    info!("  Triple Count:          {}", triple_count);
    info!("  Node Count:            {}", node_count);
    info!("  Max Triples:           {}", lifecycle_status.max_triples);
    info!("  Max Nodes:             {}", lifecycle_status.max_nodes);
    info!("  Embeddings Enabled:    {}", has_embeddings);
    info!("  Stigmergy Enabled:     true");
    info!("  Lifecycle Management:  true");
    info!("  Resilience Module:     true");
    info!("  Inference Training:    {}", engine.feedback_recorder.is_some());
    info!("═══════════════════════════════════════════════════════════════════");
    
    Ok(())
}

/// Valence Engine - Triple-based knowledge substrate
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to configuration file (TOML format)
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Server mode: http, mcp, or both (overrides config file)
    #[arg(long, env = "VALENCE_MODE")]
    mode: Option<String>,

    /// Host to bind to for HTTP mode (overrides config file)
    #[arg(long, env = "VALENCE_HOST")]
    host: Option<String>,

    /// Port to bind to for HTTP mode (overrides config file)
    #[arg(long, env = "VALENCE_PORT")]
    port: Option<u16>,

    /// PostgreSQL database URL (overrides config file)
    #[arg(long, env = "DATABASE_URL")]
    database_url: Option<String>,

    /// Data directory for embedded sled storage (overrides config file)
    #[arg(long, env = "VALENCE_DATA_DIR")]
    data_dir: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Parse CLI arguments
    let args = Args::parse();

    // Load configuration
    info!("Loading configuration...");
    let mut config = EngineConfig::load(args.config.as_deref())?;
    
    // Apply CLI overrides
    if let Some(mode) = args.mode {
        config.server.mode = mode;
    }
    if let Some(host) = args.host {
        config.server.host = host;
    }
    if let Some(port) = args.port {
        config.server.port = port;
    }
    if let Some(database_url) = args.database_url {
        apply_database_url(&mut config.storage, database_url);
    }
    #[cfg(feature = "embedded")]
    if let Some(data_dir) = args.data_dir {
        apply_data_dir(&mut config.storage, data_dir);
    }
    
    info!("Configuration loaded: mode={}, host={}, port={}", 
          config.server.mode, config.server.host, config.server.port);

    // Print startup banner
    print_startup_banner(&config);

    // Initialize the ValenceEngine with appropriate storage backend
    info!("Initializing ValenceEngine...");
    let (engine, store_type) = initialize_engine(&config.storage).await?;

    // Print engine status
    print_engine_status(&engine, &store_type).await?;

    // Keep a reference to the store for flushing on shutdown
    let store = engine.store.clone();

    // Run server based on mode
    match config.server.mode.as_str() {
        "http" => {
            info!("Starting in HTTP mode");
            run_http_server(engine, &config, &store_type).await?;
        }
        "mcp" => {
            info!("Starting in MCP mode (stdio)");
            run_mcp_server(engine).await?;
        }
        "both" => {
            info!("Starting in both HTTP + MCP mode");
            run_both_servers(engine, &config, &store_type).await?;
        }
        mode => {
            return Err(anyhow::anyhow!(
                "Invalid mode: {}. Valid modes: http, mcp, both",
                mode
            ));
        }
    }

    // Flush storage before exit to ensure durability
    info!("Flushing storage...");
    if let Err(e) = store.flush().await {
        warn!("Failed to flush storage on shutdown: {}", e);
    }

    info!("Server shut down gracefully");
    Ok(())
}

/// Apply database URL override to storage config
#[cfg(feature = "postgres")]
fn apply_database_url(storage: &mut valence_engine::config::StorageConfig, database_url: String) {
    use valence_engine::config::StorageConfig;
    
    match storage {
        StorageConfig::Postgres { url } => {
            *url = database_url;
        }
        StorageConfig::Tiered { database_url: db_url, .. } => {
            *db_url = database_url;
        }
        StorageConfig::Memory => {
            // Upgrade to Postgres when URL is provided
            *storage = StorageConfig::Postgres { url: database_url };
        }
    }
}

#[cfg(feature = "embedded")]
fn apply_data_dir(storage: &mut valence_engine::config::StorageConfig, data_dir: String) {
    use valence_engine::config::StorageConfig;
    match storage {
        StorageConfig::Embedded { path, .. } => {
            *path = data_dir;
        }
        _ => {
            // Upgrade to embedded when data dir is provided
            *storage = StorageConfig::Embedded {
                path: data_dir,
                recompute_embeddings_on_start: true,
                embedding_dimensions: 64,
            };
        }
    }
}

#[cfg(not(feature = "postgres"))]
fn apply_database_url(_storage: &mut valence_engine::config::StorageConfig, _database_url: String) {
    warn!("DATABASE_URL provided but postgres feature not enabled");
    warn!("Rebuild with --features postgres to use PostgreSQL storage");
}

/// Run HTTP server only
async fn run_http_server(engine: ValenceEngine, config: &EngineConfig, store_type: &str) -> Result<()> {
    // Create the API router
    info!("Creating API router...");
    let app = create_router_with_store_type(engine, store_type.to_string());

    // Parse address
    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port).parse()?;
    info!("Starting Valence Engine HTTP server on {}", addr);
    info!("Health endpoint available at: http://{}:{}/health", config.server.host, config.server.port);

    // Create TCP listener
    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Start server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("HTTP server shut down successfully");
    Ok(())
}

/// Run MCP server only (stdio)
async fn run_mcp_server(engine: ValenceEngine) -> Result<()> {
    use valence_engine::mcp::McpServer;
    
    let mcp_server = McpServer::new(engine);
    mcp_server.run_stdio().await?;
    
    Ok(())
}

/// Run both HTTP and MCP servers concurrently
async fn run_both_servers(engine: ValenceEngine, config: &EngineConfig, store_type: &str) -> Result<()> {
    use valence_engine::mcp::McpServer;

    // Create the API router
    let app = create_router_with_store_type(engine.clone(), store_type.to_string());
    
    // Parse address
    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port).parse()?;
    info!("Starting Valence Engine HTTP server on {}", addr);
    info!("Health endpoint available at: http://{}:{}/health", config.server.host, config.server.port);
    
    // Create TCP listener
    let listener = tokio::net::TcpListener::bind(addr).await?;
    
    // Create MCP server
    let mcp_server = McpServer::new(engine);
    
    // Run both concurrently
    tokio::select! {
        http_result = axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()) => {
            http_result?;
        }
        mcp_result = mcp_server.run_stdio() => {
            mcp_result?;
        }
    }
    
    info!("HTTP and MCP servers shut down successfully");
    Ok(())
}

/// Initialize ValenceEngine with the appropriate storage backend
/// Returns (engine, store_type_description)
async fn initialize_engine(storage_config: &valence_engine::config::StorageConfig) -> Result<(ValenceEngine, String)> {
    use valence_engine::config::StorageConfig;

    match storage_config {
        StorageConfig::Memory => {
            info!("Using in-memory storage backend");
            warn!("Data will not persist after server restart");
            Ok((ValenceEngine::new(), "memory".to_string()))
        }

        #[cfg(feature = "embedded")]
        StorageConfig::Embedded { path, recompute_embeddings_on_start, embedding_dimensions } => {
            use valence_engine::storage::{SledStore, TripleStore};
            use std::path::PathBuf;

            let db_path = PathBuf::from(path);
            info!("Initializing embedded sled storage at {:?}", db_path);

            // Create parent directories if needed
            if let Some(parent) = db_path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create data directory: {:?}", parent))?;
            }

            let store = SledStore::open(&db_path)
                .context("Failed to open sled database")?;

            // Check what we're restoring
            let triple_count = store.count_triples().await?;
            let node_count = store.count_nodes().await?;

            if triple_count > 0 {
                info!("Restored {} triples and {} nodes from sled", triple_count, node_count);
            } else {
                info!("Fresh sled database (no existing data)");
            }

            let engine = ValenceEngine::from_triple_store(store);

            // Rehydrate embeddings from persisted triples
            if *recompute_embeddings_on_start && triple_count > 0 {
                info!("Recomputing embeddings from {} persisted triples ({} dimensions)...",
                      triple_count, embedding_dimensions);
                match engine.recompute_embeddings(*embedding_dimensions).await {
                    Ok(count) => {
                        info!("Embeddings rehydrated: {} node embeddings computed", count);
                    }
                    Err(e) => {
                        warn!("Failed to recompute embeddings on startup: {}. Engine will run without embeddings.", e);
                    }
                }
            }

            Ok((engine, "sled (embedded)".to_string()))
        }

        #[cfg(feature = "postgres")]
        StorageConfig::Postgres { url } => {
            info!("Initializing PostgreSQL storage backend");
            info!("Database URL: {}", mask_password(url));

            use valence_engine::storage::PgStore;
            let store = PgStore::new(url)
                .await
                .context("Failed to initialize PostgreSQL store")?;

            info!("PostgreSQL store initialized successfully");
            Ok((ValenceEngine::from_triple_store(store), "postgres".to_string()))
        }

        #[cfg(feature = "postgres")]
        StorageConfig::Tiered {
            database_url,
            hot_capacity,
            promotion_policy,
            demotion_policy,
            demotion_interval_secs,
            track_accesses,
        } => {
            info!("Initializing tiered storage backend (hot memory + cold PostgreSQL)");
            info!("Database URL: {}", mask_password(database_url));
            info!("Hot tier capacity: {} triples", hot_capacity);

            use valence_engine::storage::PgStore;
            use valence_engine::tiered_store::{TieredStore, TieredConfig};

            // Create cold tier (PostgreSQL)
            let cold_store = PgStore::new(database_url)
                .await
                .context("Failed to initialize PostgreSQL cold store")?;

            // Create tiered config
            let tiered_config = TieredConfig {
                hot_capacity: *hot_capacity,
                promotion_policy: promotion_policy.clone().into(),
                demotion_policy: demotion_policy.clone().into(),
                demotion_interval_secs: *demotion_interval_secs,
                enable_cold_tier: true,
                track_accesses: *track_accesses,
            };

            // Create tiered store
            let store = TieredStore::new(cold_store, tiered_config);

            info!("Tiered storage initialized successfully");
            Ok((ValenceEngine::from_triple_store(store), "tiered".to_string()))
        }
    }
}

/// Mask password in database URL for logging
fn mask_password(url: &str) -> String {
    if let Some(at_pos) = url.rfind('@') {
        if let Some(colon_pos) = url[..at_pos].rfind(':') {
            let mut masked = url.to_string();
            masked.replace_range(colon_pos + 1..at_pos, "****");
            return masked;
        }
    }
    url.to_string()
}

/// Handle shutdown signals (SIGINT/SIGTERM)
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received SIGINT (Ctrl+C)");
        },
        _ = terminate => {
            info!("Received SIGTERM");
        },
    }

    info!("Shutdown signal received, starting graceful shutdown...");
}
