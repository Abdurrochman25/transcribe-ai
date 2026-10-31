//! Query tabel `transcripts` + `transcript_events` — docs/04-database-design.md.

use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::domain::transcript::TranscriptRow;

/// Cast kolom agar FromRow sederhana: enum→text, numeric→float8.
const COLUMNS: &str = "id, user_id, title, source_filename, size_bytes, mime_type, storage_key, \
     duration_seconds::float8 AS duration_seconds, language, requested_language, model, prompt_hint, \
     status::text AS status, progress, created_at, completed_at";

#[allow(clippy::too_many_arguments)]
pub async fn insert(
    db: &PgPool,
    id: Uuid,
    user_id: Uuid,
    title: &str,
    source_filename: &str,
    size_bytes: i64,
    mime_type: &str,
    storage_key: &str,
    duration_seconds: Option<f64>,
    requested_language: Option<&str>,
    model: &str,
    prompt_hint: Option<&str>,
) -> Result<TranscriptRow, sqlx::Error> {
    sqlx::query_as::<_, TranscriptRow>(&format!(
        "INSERT INTO transcripts (id, user_id, title, source_filename, size_bytes, mime_type,
            storage_key, duration_seconds, requested_language, model, prompt_hint, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'queued')
         RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(user_id)
    .bind(title)
    .bind(source_filename)
    .bind(size_bytes)
    .bind(mime_type)
    .bind(storage_key)
    .bind(duration_seconds)
    .bind(requested_language)
    .bind(model)
    .bind(prompt_hint)
    .fetch_one(db)
    .await
}

pub async fn log_event(
    db: &PgPool,
    transcript_id: Uuid,
    from_status: &str,
    to_status: &str,
    message: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO transcript_events (transcript_id, from_status, to_status, message)
         VALUES ($1, $2::transcript_status, $3::transcript_status, $4)",
    )
    .bind(transcript_id)
    .bind(from_status)
    .bind(to_status)
    .bind(message)
    .execute(db)
    .await?;
    Ok(())
}

#[derive(Debug, Default, Clone)]
pub struct ListParams {
    pub q: Option<String>,
    pub status: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

pub async fn list_for_user(
    db: &PgPool,
    user_id: Uuid,
    params: &ListParams,
) -> Result<(Vec<TranscriptRow>, i64), sqlx::Error> {
    let q_pattern = params
        .q
        .as_deref()
        .map(|q| format!("%{}%", q.replace('%', "\\%")));

    let rows = sqlx::query_as::<_, TranscriptRow>(&format!(
        "SELECT {COLUMNS} FROM transcripts
         WHERE user_id = $1 AND deleted_at IS NULL
           AND ($2::text IS NULL OR title ILIKE $2)
           AND ($3::text IS NULL OR status::text = $3)
         ORDER BY created_at DESC
         LIMIT $4 OFFSET $5"
    ))
    .bind(user_id)
    .bind(q_pattern.as_deref())
    .bind(&params.status)
    .bind(params.limit)
    .bind(params.offset)
    .fetch_all(db)
    .await?;

    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM transcripts
         WHERE user_id = $1 AND deleted_at IS NULL
           AND ($2::text IS NULL OR title ILIKE $2)
           AND ($3::text IS NULL OR status::text = $3)",
    )
    .bind(user_id)
    .bind(q_pattern.as_deref())
    .bind(&params.status)
    .fetch_one(db)
    .await?;

    Ok((rows, total.0))
}

pub async fn find_by_id_for_user(
    db: &PgPool,
    id: Uuid,
    user_id: Uuid,
) -> Result<Option<TranscriptRow>, sqlx::Error> {
    sqlx::query_as::<_, TranscriptRow>(&format!(
        "SELECT {COLUMNS} FROM transcripts WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL"
    ))
    .bind(id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}

/// Jam PATCH metadata — dipakai fase 4 (editor).
#[allow(dead_code)]
pub async fn touch(db: &PgPool, id: Uuid, now: OffsetDateTime) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE transcripts SET updated_at = $2 WHERE id = $1")
        .bind(id)
        .bind(now)
        .execute(db)
        .await?;
    Ok(())
}
