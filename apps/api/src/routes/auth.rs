//! Endpoint `/api/v1/auth/*` — register, login, refresh, logout, me.
//! Kontrak: docs/03-api-spec.md §Auth.

use std::time::Duration;

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;

use crate::{
    error::AppError,
    middleware::{auth::AuthUser, rate_limit},
    services,
    state::AppState,
};

pub const REFRESH_COOKIE: &str = "rt";

pub fn router() -> Router<AppState> {
    // Endpoint sensitif (10 permintaan/menit/IP — docs/03 §Rate Limiting).
    let limiter = rate_limit::RateLimiter::new(10, Duration::from_secs(60));
    let limited = Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .layer(axum::middleware::from_fn(move |req, next| {
            let limiter = limiter.clone();
            async move { rate_limit::guard(limiter, req, next).await }
        }));

    Router::new()
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/ping", get(ping))
        .merge(limited)
}

// ===== Handlers =====

#[derive(serde::Deserialize)]
struct RegisterBody {
    email: String,
    password: String,
    name: String,
    #[serde(default)]
    invite_code: Option<String>,
}

async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RegisterBody>,
) -> Result<Response, AppError> {
    let outcome = services::auth::register(
        &state.db,
        &state.cfg,
        services::auth::RegisterInput {
            email: body.email,
            password: body.password,
            name: body.name,
            invite_code: body.invite_code,
        },
        user_agent(&headers),
        client_ip(&headers),
    )
    .await?;

    Ok(auth_response(StatusCode::CREATED, outcome, &state.cfg))
}

#[derive(serde::Deserialize)]
struct LoginBody {
    email: String,
    password: String,
}

async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<LoginBody>,
) -> Result<Response, AppError> {
    let outcome = services::auth::login(
        &state.db,
        &state.cfg,
        &body.email,
        &body.password,
        user_agent(&headers),
        client_ip(&headers),
    )
    .await?;

    Ok(auth_response(StatusCode::OK, outcome, &state.cfg))
}

async fn refresh(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, AppError> {
    let raw = read_refresh_cookie(&headers).ok_or(AppError::Unauthenticated(
        services::auth::MSG_SESI_TIDAK_VALID,
    ))?;

    let outcome = services::auth::refresh(
        &state.db,
        &state.cfg,
        &raw,
        user_agent(&headers),
        client_ip(&headers),
    )
    .await?;

    // Respons refresh hanya tokens (tanpa objek user) — docs/03.
    let mut res = Json(json!({
        "tokens": tokens_json(&outcome.tokens),
    }))
    .into_response();
    set_refresh_cookie(&mut res, &outcome.tokens.refresh_token, &state.cfg);
    Ok(res)
}

async fn logout(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    services::auth::logout(
        &state.db,
        auth.user_id,
        read_refresh_cookie(&headers).as_deref(),
    )
    .await?;
    let mut res = StatusCode::NO_CONTENT.into_response();
    clear_refresh_cookie(&mut res, &state.cfg);
    Ok(res)
}

async fn me(State(state): State<AppState>, auth: AuthUser) -> Result<Response, AppError> {
    let user = crate::middleware::auth::require_user(&state, &auth).await?;
    Ok(Json(json!({ "user": user.to_public() })).into_response())
}

async fn ping() -> Json<serde_json::Value> {
    json!({
        "message": "pong",
        "module": "auth",
        "note": "register/login aktif sejak Fase 1",
        "version": env!("CARGO_PKG_VERSION"),
    })
    .into()
}

// ===== Cookie & helper =====

fn auth_response(
    status: StatusCode,
    outcome: services::auth::AuthOutcome,
    cfg: &crate::config::Config,
) -> Response {
    let mut res = Json(json!({
        "user": outcome.user,
        "tokens": tokens_json(&outcome.tokens),
    }))
    .into_response();
    *res.status_mut() = status;
    set_refresh_cookie(&mut res, &outcome.tokens.refresh_token, cfg);
    res
}

fn tokens_json(tokens: &services::auth::AuthTokens) -> serde_json::Value {
    json!({
        "access_token": tokens.access_token,
        "refresh_token": tokens.refresh_token,
        "expires_in": tokens.expires_in,
    })
}

/// Cookie refresh: httpOnly + SameSite=Lax + Path dibatasi /api/v1/auth (docs/08 §1).
fn set_refresh_cookie(res: &mut Response, token: &str, cfg: &crate::config::Config) {
    let mut cookie = format!(
        "{REFRESH_COOKIE}={token}; Path=/api/v1/auth; HttpOnly; SameSite=Lax; Max-Age={}",
        cfg.refresh_token_ttl_secs
    );
    if cfg.cookie_secure {
        cookie.push_str("; Secure");
    }
    if let Ok(v) = HeaderValue::from_str(&cookie) {
        res.headers_mut().append(header::SET_COOKIE, v);
    }
}

fn clear_refresh_cookie(res: &mut Response, cfg: &crate::config::Config) {
    let mut cookie =
        format!("{REFRESH_COOKIE}=; Path=/api/v1/auth; HttpOnly; SameSite=Lax; Max-Age=0");
    if cfg.cookie_secure {
        cookie.push_str("; Secure");
    }
    if let Ok(v) = HeaderValue::from_str(&cookie) {
        res.headers_mut().append(header::SET_COOKIE, v);
    }
}

fn read_refresh_cookie(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for pair in raw.split(';') {
        let pair = pair.trim();
        if let Some(value) = pair.strip_prefix(REFRESH_COOKIE) {
            let value = value.strip_prefix('=')?.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn user_agent(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
}

/// IP untuk audit token — fallback ke x-forwarded-for bila ConnectInfo tak tersedia
/// (mis. saat integration test). Nilai harus parse-able sebagai IP; selain itu diabaikan
/// supaya kolom `inet` tidak menerima input sembarangan.
fn client_ip(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|v| v.parse::<std::net::IpAddr>().is_ok())
}
