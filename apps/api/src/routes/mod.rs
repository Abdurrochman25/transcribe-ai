pub mod auth;
pub mod misc;

use axum::Router;

use crate::state::AppState;

/// Semua endpoint di bawah `/api/v1`.
pub fn v1() -> Router<AppState> {
    Router::new().merge(auth::router())
}
