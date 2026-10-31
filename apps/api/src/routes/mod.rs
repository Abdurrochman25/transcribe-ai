pub mod auth;
pub mod misc;
pub mod transcripts;

use axum::Router;

use crate::state::AppState;

/// Semua endpoint di bawah `/api/v1`.
pub fn v1(max_upload_bytes: usize) -> Router<AppState> {
    Router::new()
        .nest("/auth", auth::router())
        .nest("/transcripts", transcripts::router(max_upload_bytes))
}
