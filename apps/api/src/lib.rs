//! Router utama TranscribeAI API.
//! Urutan layer (terakhir ditambah = terluar):
//! request-id → CORS → tracing → handler — docs/02-architecture.md.

pub mod config;
pub mod domain;
pub mod error;
pub mod middleware;
pub mod repositories;
pub mod routes;
pub mod services;
pub mod state;

use axum::{
    Router,
    http::{Method, header},
    middleware as axum_middleware,
};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    trace::TraceLayer,
};

/// Membangun aplikasi Axum lengkap (dipisah dari `main` agar bisa di-test in-proc).
pub fn app(state: state::AppState) -> Router {
    let cors = build_cors(&state);

    Router::new()
        .merge(routes::misc::router())
        .nest("/api/v1", routes::v1())
        .fallback(error::fallback_404)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .layer(axum_middleware::from_fn(middleware::request_id::request_id))
        .with_state(state)
}

fn build_cors(state: &state::AppState) -> CorsLayer {
    // Fail-fast: origin tidak valid = kesalahan konfigurasi, jangan dilewati diam-diam.
    let origins: Vec<_> = state
        .cfg
        .cors_allowed_origins
        .iter()
        .map(|o| {
            o.parse()
                .unwrap_or_else(|_| panic!("CORS_ALLOWED_ORIGINS berisi origin tidak valid: {o}"))
        })
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_credentials(true)
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
}
