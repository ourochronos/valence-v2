//! Valence Engine Binary Entrypoint
//!
//! HTTP server exposing the triple store API.

use anyhow::Result;
use clap::Parser;
use std::net::SocketAddr;
use tokio::signal;
use tracing::info;
use tracing_subscriber;

use valence_engine::{api::create_router, ValenceEngine};

/// Valence Engine - Triple-based knowledge substrate
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Host to bind to
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port to bind to
    #[arg(long, default_value_t = 8421)]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Parse CLI arguments
    let args = Args::parse();

    // Initialize the ValenceEngine
    info!("Initializing ValenceEngine...");
    let engine = ValenceEngine::new();

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
