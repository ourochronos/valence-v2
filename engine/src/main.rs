//! Valence Engine Binary Entrypoint
//!
//! HTTP server exposing the triple store API.

use anyhow::Result;
#[cfg(feature = "postgres")]
use anyhow::Context;
use clap::Parser;
use std::net::SocketAddr;
use tokio::signal;
use tracing::{info, warn};

use valence_engine::{api::create_router_with_store_type, ValenceEngine};

/// Print startup banner with configuration summary
fn print_startup_banner(args: &Args) {
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
    info!("  Mode:         {}", args.mode);
    if args.mode == "http" || args.mode == "both" {
        info!("  Host:         {}", args.host);
        info!("  Port:         {}", args.port);
    }
    if let Some(ref db_url) = args.database_url {
        #[cfg(feature = "postgres")]
        {
            info!("  Database:     PostgreSQL ({})", mask_password(db_url));
        }
        #[cfg(not(feature = "postgres"))]
        {
            info!("  Database:     Memory (postgres feature not enabled)");
        }
    } else {
        info!("  Database:     Memory (volatile)");
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
    /// Server mode: http, mcp, or both
    #[arg(long, env = "VALENCE_MODE", default_value = "http")]
    mode: String,

    /// Host to bind to (for HTTP mode)
    #[arg(long, env = "VALENCE_HOST", default_value = "127.0.0.1")]
    host: String,

    /// Port to bind to (for HTTP mode)
    #[arg(long, env = "VALENCE_PORT", default_value_t = 8421)]
    port: u16,

    /// PostgreSQL database URL (optional, uses in-memory storage if not provided)
    #[arg(long, env = "DATABASE_URL")]
    database_url: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Parse CLI arguments
    let args = Args::parse();

    // Print startup banner
    print_startup_banner(&args);

    // Initialize the ValenceEngine with appropriate storage backend
    info!("Initializing ValenceEngine...");
    let (engine, store_type) = initialize_engine(args.database_url.as_deref()).await?;

    // Print engine status
    print_engine_status(&engine, &store_type).await?;

    // Run server based on mode
    match args.mode.as_str() {
        "http" => {
            info!("Starting in HTTP mode");
            run_http_server(engine, store_type, &args).await?;
        }
        "mcp" => {
            info!("Starting in MCP mode (stdio)");
            run_mcp_server(engine).await?;
        }
        "both" => {
            info!("Starting in both HTTP + MCP mode");
            run_both_servers(engine, store_type, &args).await?;
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

/// Run HTTP server only
async fn run_http_server(engine: ValenceEngine, store_type: String, args: &Args) -> Result<()> {
    // Create the API router with store type
    info!("Creating API router...");
    let app = create_router_with_store_type(engine, store_type);

    // Parse address
    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    info!("Starting Valence Engine HTTP server on {}", addr);
    info!("Health endpoint available at: http://{}:{}/health", args.host, args.port);

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
async fn run_both_servers(engine: ValenceEngine, store_type: String, args: &Args) -> Result<()> {
    use valence_engine::mcp::McpServer;
    
    // Create the API router with store type
    let app = create_router_with_store_type(engine.clone(), store_type);
    
    // Parse address
    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    info!("Starting Valence Engine HTTP server on {}", addr);
    info!("Health endpoint available at: http://{}:{}/health", args.host, args.port);
    
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
async fn initialize_engine(database_url: Option<&str>) -> Result<(ValenceEngine, String)> {
    match database_url {
        Some(_url) => {
            #[cfg(feature = "postgres")]
            {
                info!("Initializing PostgreSQL storage backend");
                info!("Database URL: {}", mask_password(url));
                
                use valence_engine::storage::PgStore;
                let store = PgStore::new(url)
                    .await
                    .context("Failed to initialize PostgreSQL store")?;
                
                info!("PostgreSQL store initialized successfully");
                Ok((ValenceEngine::from_triple_store(store), "postgres".to_string()))
            }
            
            #[cfg(not(feature = "postgres"))]
            {
                warn!("DATABASE_URL provided but postgres feature not enabled");
                warn!("Rebuild with --features postgres to use PostgreSQL storage");
                warn!("Falling back to in-memory storage");
                Ok((ValenceEngine::new(), "memory".to_string()))
            }
        }
        None => {
            info!("No DATABASE_URL provided, using in-memory storage");
            warn!("Data will not persist after server restart");
            warn!("Set DATABASE_URL environment variable or use --database-url flag for persistent storage");
            Ok((ValenceEngine::new(), "memory".to_string()))
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
