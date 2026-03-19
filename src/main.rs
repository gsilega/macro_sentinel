// Application entry point.
// Boots the Tokio async runtime, loads config, initializes shared state,
// spawns the FRED polling loop as a background task, and starts the HTTP server.

mod ai;
mod config;
mod error;
mod fred;
mod indicators;
mod routes;
mod state;
mod websocket;

use crate::config::Config;
use crate::state::AppState;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize structured logging. Set RUST_LOG=info (or debug/warn) to control verbosity.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("macro_sentinel=info".parse()?),
        )
        .init();

    info!("Starting macro_sentinel...");

    let config = Config::from_env()?;
    info!("Configuration loaded.");

    // Arc<RwLock<AppState>>: shared ownership across async tasks with concurrent read access.
    let shared_state = Arc::new(RwLock::new(AppState::new()));

    let state_for_poller = Arc::clone(&shared_state);
    let config_for_poller = config.clone();

    // Spawn the FRED polling loop as an independent background task.
    tokio::spawn(async move {
        fred::run_polling_loop(config_for_poller, state_for_poller).await;
    });

    let app = routes::build_router(shared_state, config);

    let addr = "0.0.0.0:3000";
    info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
