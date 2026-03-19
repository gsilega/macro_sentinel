// Axum router and HTTP route handlers.
//
// All routes are assembled in build_router() and registered against AppContext,
// which bundles shared state, config, and the outbound HTTP client into a single
// clonable type (required by axum's State<T> extractor).

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{State, ws::WebSocketUpgrade},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use tokio::sync::RwLock;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing::info;

use crate::ai;
use crate::config::Config;
use crate::error::AppError;
use crate::state::AppState;

/// Shared context injected into every route handler via axum's State extractor.
/// All fields clone cheaply: Arc increments a counter, Config is a small struct,
/// reqwest::Client is Arc-backed internally.
#[derive(Clone)]
pub struct AppContext {
    pub state: Arc<RwLock<AppState>>,
    pub config: Config,
    pub http_client: reqwest::Client,
}

/// Assemble the complete Axum application. Called once at startup.
pub fn build_router(state: Arc<RwLock<AppState>>, config: Config) -> Router {
    let ctx = AppContext {
        state,
        config,
        http_client: reqwest::Client::new(),
    };

    Router::new()
        .route("/api/indicators", get(get_indicators))
        .route("/api/interpret", post(post_interpret))
        .route("/api/health", get(health_check))
        .route("/ws", get(ws_handler))
        // Static files served from the `static/` directory. Registered last as catch-all.
        .nest_service("/", ServeDir::new("static"))
        .layer(CorsLayer::permissive())
        .with_state(ctx)
}

// --- Handlers ---

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "macro_sentinel is running")
}

/// GET /api/indicators — returns all current indicator readings as JSON.
async fn get_indicators(State(ctx): State<AppContext>) -> Result<impl IntoResponse, AppError> {
    let guard = ctx.state.read().await;
    let readings: Vec<_> = guard.all_latest_readings();

    Ok(Json(serde_json::json!({
        "readings": readings,
        "last_updated": guard.last_updated.map(|dt| dt.to_rfc3339()),
        "count": readings.len(),
    })))
}

/// POST /api/interpret — triggers a Claude AI analysis of current readings.
///
/// Readings are cloned and the lock released before the AI network call
/// to avoid holding the read lock across a multi-second HTTP request.
async fn post_interpret(State(ctx): State<AppContext>) -> Result<impl IntoResponse, AppError> {
    info!("AI interpretation requested");

    // Snapshot data and release the lock before the slow AI call.
    let readings_snapshot: Vec<crate::indicators::IndicatorReading> = {
        let guard = ctx.state.read().await;
        guard.all_latest_readings().into_iter().cloned().collect()
    };

    let reading_refs: Vec<_> = readings_snapshot.iter().collect();

    let interpretation = ai::interpret_indicators(
        &ctx.http_client,
        &ctx.config.anthropic_api_key,
        &reading_refs,
    )
    .await?;

    // Persist so the next WebSocket push includes the fresh interpretation.
    {
        let mut guard = ctx.state.write().await;
        guard.ai_interpretation = Some(interpretation.clone());
    }

    Ok(Json(
        serde_json::json!({ "interpretation": interpretation }),
    ))
}

/// GET /ws — upgrades the connection to WebSocket for real-time indicator pushes.
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(ctx): State<AppContext>,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| crate::websocket::handle_socket_direct(socket, ctx.state))
}
