//! Modul auth — Fase 0: hello-world endpoint.
//! Fase 1: register/login/refresh/logout/me (docs/03-api-spec.md §Auth).

use axum::{Json, Router, routing::get};
use serde_json::{Value, json};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/auth/ping", get(ping))
}

async fn ping() -> Json<Value> {
    json!({
        "message": "pong",
        "module": "auth",
        "note": "register/login aktif di Fase 1",
        "version": env!("CARGO_PKG_VERSION"),
    })
    .into()
}
