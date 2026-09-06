use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde_json::json;

use crate::state::AppState;

/// `GET /health` (FR-5.1): status, version, uptime.
pub async fn health(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": state.start_time.elapsed().as_secs(),
    }))
}
