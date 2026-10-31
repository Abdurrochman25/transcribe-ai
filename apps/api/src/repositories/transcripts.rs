//! Query tabel `transcripts` + `transcript_events` — docs/04-database-design.md.

use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::domain::transcript::TranscriptRow;

/// Cast kolom agar FromRow sederhana: enum→text, numeric→float8.
const COLUMNS: &str = "id, user_id, title, source_filename, size_bytes, mime_type, storage_key, \
     duration_seconds::float8 AS duration_seconds, language, requested_language, model, prompt_hint, \
     status::text AS status, progress, error, created_at, completed_at";

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

// ===== worker (fase 3) =====

/// Claim job berikutnya secara atomik (FOR UPDATE SKIP LOCKED — docs/04 §5).
/// Status berubah queued → validating.
pub async fn claim_next_job(db: &PgPool) -> Result<Option<TranscriptRow>, sqlx::Error> {
    sqlx::query_as::<_, TranscriptRow>(&format!(
        "UPDATE transcripts SET status = 'validating', updated_at = now()
         WHERE id = (
            SELECT id FROM transcripts
            WHERE status = 'queued' AND cancel_requested = false AND deleted_at IS NULL
            ORDER BY created_at
            FOR UPDATE SKIP LOCKED LIMIT 1
         )
         RETURNING {COLUMNS}"
    ))
    .fetch_optional(db)
    .await
}

pub async fn set_status(db: &PgPool, id: Uuid, from: &str, to: &str) -> Result<bool, sqlx::Error> {
    let res = sqlx::query(
        "UPDATE transcripts SET status = $3::transcript_status, updated_at = now()
         WHERE id = $1 AND status = $2::transcript_status",
    )
    .bind(id)
    .bind(from)
    .bind(to)
    .execute(db)
    .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn set_progress(
    db: &PgPool,
    id: Uuid,
    status: &str,
    progress: i16,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE transcripts SET status = $2::transcript_status, progress = $3, updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(status)
    .bind(progress)
    .execute(db)
    .await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct SegIn {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub confidence: Option<f32>,
}

/// Batch-insert segments dengan offset timestamp chunk (docs/02 §4).
pub async fn insert_segments(
    db: &PgPool,
    transcript_id: Uuid,
    base_idx: i32,
    offset_ms: i64,
    segments: &[SegIn],
) -> Result<(), sqlx::Error> {
    if segments.is_empty() {
        return Ok(());
    }
    let mut qb = sqlx::QueryBuilder::new(
        "INSERT INTO segments (transcript_id, idx, start_ms, end_ms, text, confidence) ",
    );
    qb.push_values(segments.iter().enumerate(), |mut b, (i, s)| {
        b.push_bind(transcript_id)
            .push_bind(base_idx + i as i32)
            .push_bind(offset_ms + s.start_ms)
            .push_bind(offset_ms + s.end_ms)
            .push_bind(&s.text)
            .push_bind(s.confidence);
    });
    qb.build().execute(db).await?;
    Ok(())
}

pub async fn delete_segments(db: &PgPool, transcript_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM segments WHERE transcript_id = $1")
        .bind(transcript_id)
        .execute(db)
        .await?;
    Ok(())
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct SegmentRow {
    pub idx: i32,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub confidence: Option<f32>,
    pub edited: bool,
}

pub async fn segments_for(
    db: &PgPool,
    transcript_id: Uuid,
) -> Result<Vec<SegmentRow>, sqlx::Error> {
    sqlx::query_as::<_, SegmentRow>(
        "SELECT idx, start_ms, end_ms, text, confidence, edited FROM segments
         WHERE transcript_id = $1 ORDER BY idx",
    )
    .bind(transcript_id)
    .fetch_all(db)
    .await
}

pub async fn finalize_completed(
    db: &PgPool,
    id: Uuid,
    language: Option<&str>,
    language_probability: Option<f32>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE transcripts
         SET status = 'completed', progress = 100, language = $2,
             language_probability = $3, completed_at = now(), updated_at = now(), error = NULL
         WHERE id = $1",
    )
    .bind(id)
    .bind(language)
    .bind(language_probability)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn fail(db: &PgPool, id: Uuid, message: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE transcripts SET status = 'failed', error = $2, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(message)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn mark_cancelled(db: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE transcripts SET status = 'cancelled', updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

/// Set cancel_requested → worker berhenti di checkpoint antar chunk (docs/02 §4).
pub async fn request_cancel(db: &PgPool, id: Uuid, user_id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query(
        "UPDATE transcripts SET cancel_requested = true, updated_at = now()
         WHERE id = $1 AND user_id = $2
           AND status IN ('queued','validating','chunking','transcribing','processing')
           AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn is_cancel_requested(db: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let row: (bool,) = sqlx::query_as("SELECT cancel_requested FROM transcripts WHERE id = $1")
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(row.0)
}

/// Ulang job gagal: failed → queued, bersihkan segments & error (docs/03 retry).
pub async fn requeue_failed(db: &PgPool, id: Uuid, user_id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query(
        "UPDATE transcripts
         SET status = 'queued', progress = 0, error = NULL, cancel_requested = false, updated_at = now()
         WHERE id = $1 AND user_id = $2 AND status = 'failed' AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Job stuck (worker mati) → kembalikan ke antrean (docs/04 §5).
pub async fn reset_stale_jobs(
    db: &PgPool,
    older_than_minutes: i64,
    exclude: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "UPDATE transcripts SET status = 'queued', progress = 0, updated_at = now()
         WHERE status IN ('validating','chunking','transcribing','processing')
           AND updated_at < now() - ($1 || ' minutes')::interval
           AND id <> ALL($2)
         RETURNING id",
    )
    .bind(older_than_minutes.to_string())
    .bind(exclude)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

/// Audio selesai > retensi hari → kembalikan storage_key untuk dihapus (docs/04 §4).
pub async fn expired_audio_keys(
    db: &PgPool,
    retention_days: i64,
) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, storage_key FROM transcripts
         WHERE completed_at < now() - ($1 || ' days')::interval
           AND storage_key <> '' AND deleted_at IS NULL",
    )
    .bind(retention_days.to_string())
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn mark_audio_purged(db: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE transcripts SET storage_key = '', updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

// ===== editor (fase 4) =====

pub async fn update_title(
    db: &PgPool,
    id: Uuid,
    user_id: Uuid,
    title: &str,
) -> Result<bool, sqlx::Error> {
    let res = sqlx::query(
        "UPDATE transcripts SET title = $3, updated_at = now()
         WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .bind(title)
    .execute(db)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Update teks satu segment + tandai edited (jejak audit ringan — docs/03 PATCH).
/// Return false bila (transcript_id, idx) tidak ditemukan.
pub async fn update_segment_text(
    db: &PgPool,
    transcript_id: Uuid,
    idx: i32,
    text: &str,
) -> Result<bool, sqlx::Error> {
    let res = sqlx::query(
        "UPDATE segments SET text = $3, edited = true
         WHERE transcript_id = $1 AND idx = $2",
    )
    .bind(transcript_id)
    .bind(idx)
    .bind(text)
    .execute(db)
    .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn count_edited(db: &PgPool, transcript_id: Uuid) -> Result<i64, sqlx::Error> {
    let row: (i64,) =
        sqlx::query_as("SELECT count(*) FROM segments WHERE transcript_id = $1 AND edited = true")
            .bind(transcript_id)
            .fetch_one(db)
            .await?;
    Ok(row.0)
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
