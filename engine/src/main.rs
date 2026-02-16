//! Valence Engine Binary Entrypoint
//!
//! HTTP server exposing the triple store API.

use anyhow::Result;
#[cfg(feature = "postgres")]
use anyhow::Context;
use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::signal;
use tracing::{info, warn};

use valence_engine::{api::create_router, ValenceEngine, EngineConfig};

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
    
    info!("Configuration loaded: mode={}, host={}, port={}", 
          config.server.mode, config.server.host, config.server.port);

    // Initialize the ValenceEngine with appropriate storage backend
    info!("Initializing ValenceEngine...");
    let engine = initialize_engine(&config.storage).await?;

    // Run server based on mode
    match config.server.mode.as_str() {
        "http" => {
            info!("Starting in HTTP mode");
            run_http_server(engine, &config).await?;
        }
        "mcp" => {
            info!("Starting in MCP mode (stdio)");
            run_mcp_server(engine).await?;
        }
        "both" => {
            info!("Starting in both HTTP + MCP mode");
            run_both_servers(engine, &config).await?;
        }
        mode => {
            return Err(anyhow::anyhow!(
                "Invalid mode: {}. Valid modes: http, mcp, both",
                mode
            ));
        }
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

#[cfg(not(feature = "postgres"))]
fn apply_database_url(_storage: &mut valence_engine::config::StorageConfig, _database_url: String) {
    warn!("DATABASE_URL provided but postgres feature not enabled");
    warn!("Rebuild with --features postgres to use PostgreSQL storage");
}

/// Run HTTP server only
async fn run_http_server(engine: ValenceEngine, config: &EngineConfig) -> Result<()> {
    // Create the API router
    info!("Creating API router...");
    let app = create_router(engine);

    // Parse address
    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port).parse()?;
    info!("Starting Valence Engine HTTP server on {}", addr);

    // Create TCP listener
    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Start server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

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
async fn run_both_servers(engine: ValenceEngine, config: &EngineConfig) -> Result<()> {
    use valence_engine::mcp::McpServer;
    
    // Create the API router
    let app = create_router(engine.clone());
    
    // Parse address
    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port).parse()?;
    info!("Starting Valence Engine HTTP server on {}", addr);
    
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
    
    Ok(())
}

/// Initialize ValenceEngine with the appropriate storage backend
async fn initialize_engine(storage_config: &valence_engine::config::StorageConfig) -> Result<ValenceEngine> {
    use valence_engine::config::StorageConfig;
    
    match storage_config {
        StorageConfig::Memory => {
            info!("Using in-memory storage backend");
            warn!("Data will not persist after server restart");
            Ok(ValenceEngine::new())
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
            Ok(ValenceEngine::from_triple_store(store))
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
            Ok(ValenceEngine::from_triple_store(store))
        }
    }
}

/// Mask password in database URL for logging
#[cfg(feature = "postgres")]
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
