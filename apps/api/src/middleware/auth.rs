//! Extractor `AuthUser` — validasi Bearer JWT di layer extract (docs/05 §2).
//! Handler cukup `user: AuthUser` untuk endpoint 🔒.

use axum::{
    extract::{FromRequestParts, Request},
    http::request::Parts,
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

use crate::{
    domain::user::UserRow, error::AppError, repositories, services::auth::MSG_PERLU_LOGIN,
    state::AppState,
};

#[derive(Debug, Clone, Copy)]
pub struct AuthUser {
    pub user_id: Uuid,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(AppError::Unauthenticated(MSG_PERLU_LOGIN))?;

        let claims = jsonwebtoken::decode::<crate::services::auth::Claims>(
            token,
            &jsonwebtoken::DecodingKey::from_secret(state.cfg.jwt_secret.as_bytes()),
            &jsonwebtoken::Validation::default(),
        )
        .map_err(|_| AppError::Unauthenticated(MSG_PERLU_LOGIN))?;

        let user_id = Uuid::parse_str(&claims.claims.sub)
            .map_err(|_| AppError::Unauthenticated(MSG_PERLU_LOGIN))?;

        Ok(AuthUser { user_id })
    }
}

/// Convenience: user lengkap (dengan data terbaru dari DB) untuk endpoint seperti /auth/me.
pub async fn require_user(state: &AppState, auth: &AuthUser) -> Result<UserRow, AppError> {
    repositories::users::find_by_id(&state.db, auth.user_id)
        .await?
        .ok_or(AppError::Unauthenticated(
            crate::services::auth::MSG_PERLU_LOGIN,
        ))
}

// Import di bawah hanya menjaga signature middleware konsisten bila guard
// role-based ditambahkan fase berikutnya.
#[allow(dead_code)]
async fn _unused_middleware_shape(_req: Request, _next: Next) -> Response {
    unreachable!()
}
