//! Endpoint `/api/v1/transcripts` — upload (multipart) & list.
//! Kontrak: docs/03-api-spec.md §Transcripts.

use axum::{
    Json, Router,
    extract::{Multipart, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;
use tokio::io::AsyncWriteExt;

use crate::{
    error::{AppError, FieldError},
    middleware::auth::AuthUser,
    services::{self, transcript::UploadMeta},
    state::AppState,
};

pub fn router(max_upload_bytes: usize) -> Router<AppState> {
    Router::new()
        .route("/", post(create).get(list))
        .route("/{id}", get(detail))
        .route("/{id}/events", get(events))
        .route("/{id}/cancel", post(cancel))
        .route("/{id}/retry", post(retry))
        // multipart body limit = ukuran file + margin field teks (docs/08 §2)
        .layer(axum::extract::DefaultBodyLimit::max(
            max_upload_bytes.saturating_add(64 * 1024),
        ))
}

// ===== POST /transcripts (upload) =====

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    // Rate limit upload per user (docs/03 §Rate Limiting).
    if let Err(retry_after) = state
        .upload_limiter
        .check(&format!("upload:{}", auth.user_id))
    {
        let mut res = AppError::RateLimited.into_response();
        if let Ok(v) = HeaderValue::from_str(&retry_after.to_string()) {
            res.headers_mut().insert(header::RETRY_AFTER, v);
        }
        return Ok(res);
    }

    let temp_dir = tempfile::tempdir()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("temp dir gagal: {e}")))?;
    let temp_path = temp_dir.path().join("upload.bin");

    let mut meta = UploadMeta {
        temp_path: temp_path.clone(),
        ..Default::default()
    };
    let mut file_seen = false;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        AppError::Validation(vec![FieldError {
            field: "form".into(),
            issue: format!("multipart tidak valid: {e}"),
        }])
    })? {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "file" => {
                meta.original_filename = field.file_name().unwrap_or("upload").trim().to_string();
                stream_to_temp(&state, field, &temp_path).await?;
                file_seen = true;
            }
            "title" => meta.title = non_empty(field.text().await.ok()),
            "language" => meta.requested_language = non_empty(field.text().await.ok()),
            "model" => meta.model = non_empty(field.text().await.ok()),
            "prompt" => meta.prompt_hint = non_empty(field.text().await.ok()),
            _ => {} // field tak dikenal diabaikan
        }
    }

    if !file_seen {
        return Err(AppError::Validation(vec![FieldError {
            field: "file".into(),
            issue: "field `file` wajib diisi".into(),
        }]));
    }

    let row = services::transcript::create(&state, auth.user_id, meta).await?;
    let mut res = Json(json!({ "transcript": row.to_public() })).into_response();
    *res.status_mut() = StatusCode::ACCEPTED;
    Ok(res)
}

/// Stream field multipart ke file temp dengan penegakan batas ukuran.
async fn stream_to_temp(
    state: &AppState,
    mut field: axum::extract::multipart::Field<'_>,
    temp_path: &std::path::Path,
) -> Result<(), AppError> {
    let mut file = tokio::fs::File::create(temp_path)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("buat temp file: {e}")))?;
    let mut written: usize = 0;

    while let Some(chunk) = field.chunk().await.map_err(|e| {
        AppError::Validation(vec![FieldError {
            field: "file".into(),
            issue: format!("upload terputus: {e}"),
        }])
    })? {
        written += chunk.len();
        if written > state.cfg.max_upload_bytes {
            // docs/08 §2: ditolak sebelum dibuffer penuh.
            return Err(AppError::TooLarge);
        }
        file.write_all(&chunk)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("tulis temp file: {e}")))?;
    }
    file.flush()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("flush temp file: {e}")))?;
    Ok(())
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

// ===== GET /transcripts/{id} (detail) =====

async fn detail(
    State(state): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
) -> Result<Response, AppError> {
    let row = crate::repositories::transcripts::find_by_id_for_user(&state.db, id, auth.user_id)
        .await?
        .ok_or(AppError::NotFound)?;

    // Segments hanya tersedia setelah selesai (docs/03).
    let segments = if row.status == "completed" {
        crate::repositories::transcripts::segments_for(&state.db, id).await?
    } else {
        vec![]
    };

    // TODO(fase 4): audio_url presigned untuk player editor.
    Ok(Json(json!({
        "transcript": row.to_public(),
        "segments": segments,
    }))
    .into_response())
}

// ===== GET /transcripts/{id}/events (SSE) =====

/// EventSource tidak bisa mengirim header Authorization → terima juga
/// `?token=<access_jwt>` (TTL 15 menit) sebagai alternatif Bearer.
async fn events(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let token: Option<String> = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| v.to_string())
        .or_else(|| query.get("token").cloned());

    let Some(token) = token else {
        return AppError::Unauthenticated(crate::services::auth::MSG_PERLU_LOGIN).into_response();
    };

    let claims = jsonwebtoken::decode::<crate::services::auth::Claims>(
        &token,
        &jsonwebtoken::DecodingKey::from_secret(state.cfg.jwt_secret.as_bytes()),
        &jsonwebtoken::Validation::default(),
    );
    let user_id = match claims {
        Ok(c) => uuid::Uuid::parse_str(&c.claims.sub).ok(),
        Err(_) => None,
    };
    let Some(user_id) = user_id else {
        return AppError::Unauthenticated(crate::services::auth::MSG_PERLU_LOGIN).into_response();
    };

    let row =
        match crate::repositories::transcripts::find_by_id_for_user(&state.db, id, user_id).await {
            Ok(Some(row)) => row,
            _ => return AppError::NotFound.into_response(),
        };

    use axum::response::sse::{Event, KeepAlive, Sse};
    use futures::StreamExt;

    let initial_payload = match row.status.as_str() {
        "completed" => crate::infra::sse::SsePayload::Completed {
            transcript_id: row.id,
            duration_seconds: row.duration_seconds,
            language: row.language.clone(),
        },
        "failed" => crate::infra::sse::SsePayload::Failed {
            message: row.error.clone().unwrap_or_else(|| "gagal".into()),
        },
        "cancelled" => crate::infra::sse::SsePayload::Cancelled,
        status => crate::infra::sse::SsePayload::Progress {
            status: status.into(),
            progress: row.progress,
            message: None,
        },
    };

    let hub = state.sse.clone();
    let stream = futures::stream::iter([initial_payload])
        .chain(hub.subscribe(id))
        .map(|payload| {
            let kind = match &payload {
                crate::infra::sse::SsePayload::Progress { .. } => "progress",
                crate::infra::sse::SsePayload::Completed { .. } => "completed",
                crate::infra::sse::SsePayload::Failed { .. } => "failed",
                crate::infra::sse::SsePayload::Cancelled => "cancelled",
            };
            let data = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into());
            Ok::<Event, std::convert::Infallible>(Event::default().event(kind).data(data))
        });

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

// ===== POST /transcripts/{id}/cancel & /retry =====

async fn cancel(
    State(state): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
) -> Result<Response, AppError> {
    let ok = crate::repositories::transcripts::request_cancel(&state.db, id, auth.user_id)
        .await
        .map_err(AppError::from)?;
    if !ok {
        return Err(AppError::Conflict("job sudah final atau bukan milik Anda"));
    }
    tracing::info!(transcript_id = %id, "cancel diminta user");
    Ok(StatusCode::ACCEPTED.into_response())
}

async fn retry(
    State(state): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
) -> Result<Response, AppError> {
    let ok = crate::repositories::transcripts::requeue_failed(&state.db, id, auth.user_id)
        .await
        .map_err(AppError::from)?;
    if !ok {
        return Err(AppError::Conflict("hanya job gagal yang bisa diulang"));
    }
    let _ = crate::repositories::transcripts::log_event(
        &state.db,
        id,
        "failed",
        "queued",
        "diulang oleh user",
    )
    .await;
    Ok(Json(json!({ "ok": true, "status": "queued" })).into_response())
}

// ===== GET /transcripts (list) =====

#[derive(Debug, Deserialize)]
struct ListQuery {
    page: Option<i64>,
    per_page: Option<i64>,
    status: Option<String>,
    q: Option<String>,
}

const STATUSES: &[&str] = &[
    "uploaded",
    "queued",
    "validating",
    "chunking",
    "transcribing",
    "processing",
    "completed",
    "failed",
    "cancelled",
];

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> Result<Response, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);

    let status = match query
        .status
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        None => None,
        Some(s) if STATUSES.contains(&s.to_lowercase().as_str()) => Some(s.to_lowercase()),
        Some(_) => {
            return Err(AppError::Validation(vec![FieldError {
                field: "status".into(),
                issue: "status tidak valid".into(),
            }]));
        }
    };
    let q = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);

    let params = crate::repositories::transcripts::ListParams {
        q,
        status,
        limit: per_page,
        offset: (page - 1) * per_page,
    };
    let (rows, total) =
        crate::repositories::transcripts::list_for_user(&state.db, auth.user_id, &params).await?;

    let total_pages = if total == 0 {
        0
    } else {
        (total + per_page - 1) / per_page
    };
    let items: Vec<_> = rows.iter().map(|r| r.to_public()).collect();

    Ok(Json(json!({
        "items": items,
        "page": page,
        "per_page": per_page,
        "total": total,
        "total_pages": total_pages,
    }))
    .into_response())
}
