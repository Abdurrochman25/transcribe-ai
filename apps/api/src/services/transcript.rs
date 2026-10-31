//! Orkestrasi upload transcript (docs/02 §3.1, docs/08 §2):
//! temp-file → sniff magic bytes → ffprobe → simpan ke object storage → INSERT row.

use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::domain::transcript::TranscriptRow;
use crate::error::{AppError, FieldError};
use crate::infra;
use crate::repositories;
use crate::state::AppState;

pub const MAX_TITLE_LEN: usize = 200;
pub const MAX_PROMPT_LEN: usize = 500;

#[derive(Debug, Default)]
pub struct UploadMeta {
    pub temp_path: PathBuf,
    pub original_filename: String,
    pub title: Option<String>,
    pub requested_language: Option<String>,
    pub model: Option<String>,
    pub prompt_hint: Option<String>,
}

pub async fn create(
    state: &AppState,
    user_id: Uuid,
    meta: UploadMeta,
) -> Result<TranscriptRow, AppError> {
    let mut errors = Vec::new();

    // Validasi opsi.
    let model = meta
        .model
        .as_deref()
        .unwrap_or(crate::domain::transcript::MODEL_TURBO);
    if !crate::domain::transcript::allowed_models().contains(&model) {
        errors.push(FieldError {
            field: "model".into(),
            issue: "model tidak dikenal".into(),
        });
    }
    if let Some(lang) = &meta.requested_language
        && (lang.len() > 8 || !lang.chars().all(char::is_alphanumeric))
    {
        errors.push(FieldError {
            field: "language".into(),
            issue: "kode bahasa tidak valid (ISO-639-1)".into(),
        });
    }
    if let Some(p) = &meta.prompt_hint
        && p.chars().count() > MAX_PROMPT_LEN
    {
        errors.push(FieldError {
            field: "prompt".into(),
            issue: format!("maksimal {MAX_PROMPT_LEN} karakter"),
        });
    }
    let title = sanitized_title(meta.title.as_deref(), &meta.original_filename);
    if title.is_empty() || title.chars().count() > MAX_TITLE_LEN {
        errors.push(FieldError {
            field: "title".into(),
            issue: "1-200 karakter".into(),
        });
    }
    if !errors.is_empty() {
        return Err(AppError::Validation(errors));
    }

    // 1) Magic bytes — ekstensi/content-type klien tidak dipercaya (docs/08 §2).
    let prefix = read_prefix(&meta.temp_path).await?;
    let mime = infra::media::sniff_mime(&prefix).ok_or_else(|| {
        AppError::Validation(vec![FieldError {
            field: "file".into(),
            issue: format!(
                "tipe file tidak didukung (terdeteksi bukan audio/video yang diizinkan) — nama: {}",
                meta.original_filename
            ),
        }])
    })?;

    // 2) ffprobe — pastikan media valid & ambil durasi.
    let probe = infra::ffmpeg::probe(&state.cfg.ffprobe_path, &meta.temp_path)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "ffprobe gagal");
            AppError::Validation(vec![FieldError {
                field: "file".into(),
                issue: "file tidak bisa dibaca sebagai media audio/video".into(),
            }])
        })?;

    // 3) Simpan ke object storage.
    let id = Uuid::now_v7();
    let storage_key = format!("users/{user_id}/{id}/source");
    let file = tokio::fs::File::open(&meta.temp_path)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("buka temp file: {e}")))?;
    state
        .storage
        .put_file(&storage_key, file)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("upload ke storage gagal: {e}")))?;

    // 4) INSERT row (status langsung queued — file sudah tersimpan, docs/04 §5).
    let size_bytes = tokio::fs::metadata(&meta.temp_path)
        .await
        .map(|m| m.len() as i64)
        .unwrap_or(0);
    let row = repositories::transcripts::insert(
        &state.db,
        id,
        user_id,
        &title,
        &meta.original_filename,
        size_bytes,
        mime,
        &storage_key,
        probe.duration_seconds,
        meta.requested_language.as_deref(),
        model,
        meta.prompt_hint.as_deref(),
    )
    .await?;

    repositories::transcripts::log_event(
        &state.db,
        id,
        "uploaded",
        "queued",
        &format!(
            "upload selesai ({}, {} detik) — masuk antrean",
            infra::media::mime_label(mime),
            probe
                .duration_seconds
                .map(|d| format!("{d:.0}"))
                .unwrap_or_else(|| "?".into())
        ),
    )
    .await?;

    tracing::info!(transcript_id = %id, mime, size_bytes, "transcript diupload & masuk antrean");
    Ok(row)
}

/// Baca 64 byte pertama untuk deteksi signature.
async fn read_prefix(path: &Path) -> Result<Vec<u8>, AppError> {
    use tokio::io::AsyncReadExt;
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("buka temp file: {e}")))?;
    let mut buf = vec![0u8; 64];
    let n = file
        .read(&mut buf)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("baca temp file: {e}")))?;
    buf.truncate(n);
    Ok(buf)
}

/// Judul akhir: opsi user → nama file (tanpa ekstensi) → fallback.
fn sanitized_title(title: Option<&str>, original: &str) -> String {
    let from_user = title.map(str::trim).filter(|t| !t.is_empty());
    let from_file = std::path::Path::new(original)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::trim)
        .filter(|t| !t.is_empty());
    from_user
        .or(from_file)
        .unwrap_or("Transkrip tanpa judul")
        .chars()
        .take(MAX_TITLE_LEN)
        .collect()
}
