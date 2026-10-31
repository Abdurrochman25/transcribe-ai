//! Endpoint utilitas: `/health` (liveness) dan `/ready` (readiness).
//! Lihat docs/03-api-spec.md §Utilities.

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
}

/// Liveness — selalu 200 selama proses hidup.
async fn health() -> Json<Value> {
    json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    })
    .into()
}

/// Readiness — 200 bila dependensi siap, 503 bila tidak.
async fn ready(State(state): State<AppState>) -> Response {
    let db_ok = sqlx::query("SELECT 1").execute(&state.db).await.is_ok();
    // TODO(fase 2): tambah pemeriksaan object storage (HEAD bucket).
    let body = json!({ "db": db_ok, "storage": Value::Null });
    let status = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, Json(body)).into_response()
}
