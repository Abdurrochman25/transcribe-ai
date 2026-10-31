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
        .route("/{id}", get(detail).patch(patch))
        .route("/{id}/audio", get(audio))
        .route("/{id}/events", get(events))
        .route("/{id}/export", get(export))
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

/// Respons detail dipakai GET & PATCH (docs/03 — PATCH mengembalikan detail terbaru).
async fn detail_response(
    state: &AppState,
    id: uuid::Uuid,
    user_id: uuid::Uuid,
) -> Result<Response, AppError> {
    let row = crate::repositories::transcripts::find_by_id_for_user(&state.db, id, user_id)
        .await?
        .ok_or(AppError::NotFound)?;

    // Segments hanya tersedia setelah selesai (docs/03).
    let segments = if row.status == "completed" {
        crate::repositories::transcripts::segments_for(&state.db, id).await?
    } else {
        vec![]
    };

    // Player audio: endpoint streaming ber-auth (klien menambah `?token=`).
    // Null bila audio sudah dihapus oleh retensi/cancel.
    let audio_url =
        (!row.storage_key.is_empty()).then(|| format!("/api/v1/transcripts/{id}/audio"));

    Ok(Json(json!({
        "transcript": row.to_public(),
        "segments": segments,
        "audio_url": audio_url,
    }))
    .into_response())
}

async fn detail(
    State(state): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
) -> Result<Response, AppError> {
    detail_response(&state, id, auth.user_id).await
}

// ===== PATCH /transcripts/{id} (editor) =====

#[derive(Deserialize)]
struct PatchBody {
    title: Option<String>,
    segments: Option<Vec<PatchSegment>>,
}

#[derive(Deserialize)]
struct PatchSegment {
    idx: i32,
    text: String,
}

async fn patch(
    State(state): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
    Json(body): Json<PatchBody>,
) -> Result<Response, AppError> {
    let row = crate::repositories::transcripts::find_by_id_for_user(&state.db, id, auth.user_id)
        .await?
        .ok_or(AppError::NotFound)?;

    // Validasi judul (dipakai list/search — jaga wajar).
    if let Some(t) = &body.title {
        let t = t.trim();
        if t.is_empty() || t.chars().count() > 200 {
            return Err(AppError::Validation(vec![FieldError {
                field: "title".into(),
                issue: "judul wajib 1–200 karakter".into(),
            }]));
        }
    }

    // Edit segment hanya saat completed (docs/03) → selain itu 409.
    if body.segments.is_some() && row.status != "completed" {
        return Err(AppError::Conflict(
            "segment hanya bisa diedit setelah transkripsi selesai",
        ));
    }
    for s in body.segments.iter().flatten() {
        let t = s.text.trim();
        if t.is_empty() || t.chars().count() > 5000 {
            return Err(AppError::Validation(vec![FieldError {
                field: "segments".into(),
                issue: format!("teks segment idx={} wajib 1–5000 karakter", s.idx),
            }]));
        }
    }

    if let Some(title) = &body.title {
        crate::repositories::transcripts::update_title(&state.db, id, auth.user_id, title.trim())
            .await?;
    }
    if let Some(segs) = &body.segments {
        let mut missing = Vec::new();
        for s in segs {
            let ok = crate::repositories::transcripts::update_segment_text(
                &state.db,
                id,
                s.idx,
                s.text.trim(),
            )
            .await?;
            if !ok {
                missing.push(s.idx);
            }
        }
        if !missing.is_empty() {
            tracing::warn!(transcript_id = %id, ?missing, "segment tidak ditemukan saat PATCH");
        }
        let _ = crate::repositories::transcripts::log_event(
            &state.db,
            id,
            "completed",
            "completed",
            &format!("{} segment diedit user", segs.len()),
        )
        .await;
    }
    tracing::info!(transcript_id = %id, "transcript diedit user");

    detail_response(&state, id, auth.user_id).await
}

// ===== GET /transcripts/{id}/audio (streaming + HTTP Range) =====

/// Elemen `<audio>` butuh dukungan Range agar seek berfungsi (RFC 7233).
/// `bytes=a-b` | `bytes=a-` | `bytes=-n` (suffix). Di-clamp ke ukuran file;
/// range tak satisfiable → fallback 200 penuh (sederhana & aman untuk player).
fn parse_range(h: &str, size: u64) -> Option<(u64, u64)> {
    let spec = h.strip_prefix("bytes=")?.trim();
    let (a, b) = spec.split_once('-')?;
    if a.is_empty() {
        // Suffix: N byte terakhir.
        let n: u64 = b.parse().ok()?;
        if n == 0 || size == 0 {
            return None;
        }
        let n = n.min(size);
        Some((size - n, size - 1))
    } else {
        let start: u64 = a.parse().ok()?;
        if start >= size {
            return None;
        }
        let end = match b.parse::<u64>() {
            Ok(e) if e >= start => e.min(size - 1),
            Ok(_) => return None,
            Err(_) => size - 1, // "a-" → sampai akhir
        };
        Some((start, end))
    }
}

async fn audio(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let Some(user_id) = token_or_bearer_user_id(&state, &headers, &query) else {
        return AppError::Unauthenticated(crate::services::auth::MSG_PERLU_LOGIN).into_response();
    };
    let row =
        match crate::repositories::transcripts::find_by_id_for_user(&state.db, id, user_id).await {
            Ok(Some(r)) => r,
            _ => return AppError::NotFound.into_response(),
        };
    if row.storage_key.is_empty() {
        return AppError::NotFound.into_response(); // audio dihapus retensi/cancel
    }

    let size = match state.storage.head_size(&row.storage_key).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(transcript_id = %id, error = %e, "audio hilang dari storage");
            return AppError::NotFound.into_response();
        }
    };

    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|h| parse_range(h, size));

    let builder = match range {
        Some((start, end)) => {
            let bytes = match state.storage.get_range(&row.storage_key, start, end).await {
                Ok(b) => b,
                Err(e) => {
                    return AppError::Internal(anyhow::anyhow!("baca audio gagal: {e}"))
                        .into_response();
                }
            };
            let last = start + bytes.len() as u64 - 1;
            Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(
                    header::CONTENT_RANGE,
                    format!("bytes {start}-{last}/{size}"),
                )
                .header(header::CONTENT_LENGTH, bytes.len())
                .body(axum::body::Body::from(bytes))
        }
        None => {
            // Tanpa Range → stream penuh tanpa buffering memori.
            let stream = match state.storage.get_stream(&row.storage_key).await {
                Ok(s) => s,
                Err(e) => {
                    return AppError::Internal(anyhow::anyhow!("buka audio gagal: {e}"))
                        .into_response();
                }
            };
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_LENGTH, size)
                .body(axum::body::Body::from_stream(stream))
        }
    };

    builder
        .map(|res| {
            let (mut parts, body) = res.into_parts();
            parts
                .headers
                .insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
            parts.headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_str(&row.mime_type)
                    .unwrap_or(HeaderValue::from_static("application/octet-stream")),
            );
            Response::from_parts(parts, body)
        })
        .unwrap_or_else(|e| {
            AppError::Internal(anyhow::anyhow!("respons audio gagal: {e}")).into_response()
        })
}

// ===== GET /transcripts/{id}/export =====

#[derive(Deserialize)]
struct ExportQuery {
    format: Option<String>,
}

async fn export(
    State(state): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
    axum::extract::Query(q): axum::extract::Query<ExportQuery>,
) -> Result<Response, AppError> {
    let Some(format) = q
        .format
        .as_deref()
        .and_then(services::export::ExportFormat::parse)
    else {
        return Err(AppError::Validation(vec![FieldError {
            field: "format".into(),
            issue: "harus salah satu: txt, srt, vtt, json".into(),
        }]));
    };

    let row = crate::repositories::transcripts::find_by_id_for_user(&state.db, id, auth.user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if row.status != "completed" {
        return Err(AppError::Conflict(
            "export hanya untuk transkripsi yang selesai",
        ));
    }

    let segs = crate::repositories::transcripts::segments_for(&state.db, id).await?;
    let body = services::export::render(format, &row, &services::export::from_repo(&segs));
    let filename = format!("{}.{}", services::export::slugify(&row.title), format.ext());

    let res = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, format.mime())
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .body(axum::body::Body::from(body));
    res.map_err(|e| AppError::Internal(anyhow::anyhow!("respons export gagal: {e}")))
}

// ===== GET /transcripts/{id}/events (SSE) =====

/// EventSource/`<audio>` tidak bisa mengirim header Authorization → terima juga
/// `?token=<access_jwt>` (TTL 15 menit) sebagai alternatif Bearer (dipakai events & audio).
fn token_or_bearer_user_id(
    state: &AppState,
    headers: &HeaderMap,
    query: &std::collections::HashMap<String, String>,
) -> Option<uuid::Uuid> {
    let token: Option<String> = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| v.to_string())
        .or_else(|| query.get("token").cloned());

    let token = token?;
    let claims = jsonwebtoken::decode::<crate::services::auth::Claims>(
        &token,
        &jsonwebtoken::DecodingKey::from_secret(state.cfg.jwt_secret.as_bytes()),
        &jsonwebtoken::Validation::default(),
    );
    match claims {
        Ok(c) => uuid::Uuid::parse_str(&c.claims.sub).ok(),
        Err(_) => None,
    }
}

async fn events(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let Some(user_id) = token_or_bearer_user_id(&state, &headers, &query) else {
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
