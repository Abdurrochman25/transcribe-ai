//! Error envelope konsisten — lihat docs/03-api-spec.md (Format Error).
//! Semua respons error berbentuk:
//! ```json
//! { "error": { "code": "NOT_FOUND", "message": "...", "request_id": "..." } }
//! ```
//! `request_id` disuntik oleh middleware `request_id`.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct FieldError {
    pub field: String,
    pub issue: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Unauthenticated(&'static str),
    #[error("akses ditolak")]
    Forbidden,
    #[error("resource tidak ditemukan")]
    NotFound,
    #[error("konflik: {0}")]
    Conflict(&'static str),
    #[error("request tidak valid")]
    Validation(Vec<FieldError>),
    #[error("payload terlalu besar")]
    TooLarge,
    #[error("terlalu banyak permintaan")]
    RateLimited,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    /// (status, code, message-publik, details)
    fn parts(&self) -> (StatusCode, &'static str, String, Option<Vec<FieldError>>) {
        match self {
            AppError::Unauthenticated(message) => (
                StatusCode::UNAUTHORIZED,
                "UNAUTHENTICATED",
                (*message).into(),
                None,
            ),
            AppError::Forbidden => (
                StatusCode::FORBIDDEN,
                "FORBIDDEN",
                "Akses ditolak".into(),
                None,
            ),
            AppError::NotFound => (
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                "Resource tidak ditemukan".into(),
                None,
            ),
            AppError::Conflict(detail) => (
                StatusCode::CONFLICT,
                "CONFLICT",
                format!("Konflik: {detail}"),
                None,
            ),
            AppError::Validation(details) => (
                StatusCode::BAD_REQUEST,
                "VALIDATION_ERROR",
                "Request tidak valid".into(),
                Some(details.clone()),
            ),
            AppError::TooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "PAYLOAD_TOO_LARGE",
                "Ukuran file melebihi batas".into(),
                None,
            ),
            AppError::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                "RATE_LIMITED",
                "Terlalu banyak permintaan".into(),
                None,
            ),
            // Internal: detail teknis TIDAK dibocorkan ke klien (docs/08 §4).
            AppError::Internal(err) => {
                tracing::error!(error = ?err, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL",
                    "Terjadi kesalahan internal. Silakan coba lagi nanti.".into(),
                    None,
                )
            }
        }
    }
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<&'a [FieldError]>,
}

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    error: ErrorBody<'a>,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message, details) = self.parts();
        let body = ErrorEnvelope {
            error: ErrorBody {
                code,
                message,
                details: details.as_deref(),
            },
        };
        (status, Json(body)).into_response()
    }
}

/// Fallback router 404 → envelope konsisten.
pub async fn fallback_404() -> AppError {
    AppError::NotFound
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        Self::Internal(err.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    async fn body_json(res: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .expect("body");
        serde_json::from_slice(&bytes).expect("json")
    }

    #[tokio::test]
    async fn envelope_format_dan_status() {
        let res = AppError::NotFound.into_response();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let v = body_json(res).await;
        assert_eq!(v["error"]["code"], "NOT_FOUND");

        let res = AppError::Unauthenticated("Autentikasi diperlukan").into_response();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(body_json(res).await["error"]["code"], "UNAUTHENTICATED");

        let res = AppError::Conflict("job sudah final").into_response();
        assert_eq!(res.status(), StatusCode::CONFLICT);
        assert_eq!(body_json(res).await["error"]["code"], "CONFLICT");

        let res = AppError::TooLarge.into_response();
        assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);

        let res = AppError::RateLimited.into_response();
        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn validation_menyertakan_details() {
        let res = AppError::Validation(vec![FieldError {
            field: "file".into(),
            issue: "tipe file tidak didukung".into(),
        }])
        .into_response();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let v = body_json(res).await;
        assert_eq!(v["error"]["code"], "VALIDATION_ERROR");
        assert_eq!(v["error"]["details"][0]["field"], "file");
    }

    #[tokio::test]
    async fn internal_tidak_membocorkan_detail() {
        let res = AppError::Internal(anyhow::anyhow!("db password salah: hunter2")).into_response();
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let v = body_json(res).await;
        let msg = v["error"]["message"].as_str().unwrap();
        assert!(!msg.contains("hunter2"), "detail teknis tidak boleh bocor");
        assert_eq!(v["error"]["code"], "INTERNAL");
    }
}
