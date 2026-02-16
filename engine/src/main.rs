//! Valence Engine Binary Entrypoint
//!
//! HTTP server exposing the triple store API.

use anyhow::Result;
use clap::Parser;
use std::net::SocketAddr;
use tokio::signal;
use tracing::{info, warn};

use valence_engine::{api::create_router, ValenceEngine};

/// Valence Engine - Triple-based knowledge substrate
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Host to bind to
    #[arg(long, env = "VALENCE_HOST", default_value = "127.0.0.1")]
    host: String,

    /// Port to bind to
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

    // Initialize the ValenceEngine with appropriate storage backend
    info!("Initializing ValenceEngine...");
    let engine = initialize_engine(args.database_url.as_deref()).await?;

    // Create the API router
    info!("Creating API router...");
    let app = create_router(engine);

    // Parse address
    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    info!("Starting Valence Engine server on {}", addr);

    // Create TCP listener
    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Start server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Server shut down gracefully");
    Ok(())
}

/// Initialize ValenceEngine with the appropriate storage backend
async fn initialize_engine(database_url: Option<&str>) -> Result<ValenceEngine> {
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
                Ok(ValenceEngine::from_triple_store(store))
            }
            
            #[cfg(not(feature = "postgres"))]
            {
                warn!("DATABASE_URL provided but postgres feature not enabled");
                warn!("Rebuild with --features postgres to use PostgreSQL storage");
                warn!("Falling back to in-memory storage");
                Ok(ValenceEngine::new())
            }
        }
        None => {
            info!("No DATABASE_URL provided, using in-memory storage");
            warn!("Data will not persist after server restart");
            warn!("Set DATABASE_URL environment variable or use --database-url flag for persistent storage");
            Ok(ValenceEngine::new())
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
