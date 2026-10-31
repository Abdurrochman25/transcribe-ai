//! Pipeline transkripsi per job — docs/02 §3.1 & docs/05 §4.
//! validating → (chunking) → transcribing → processing → completed | failed | cancelled

use std::path::Path;

use time::OffsetDateTime;
use uuid::Uuid;

use crate::domain::transcript::TranscriptRow;
use crate::infra;
use crate::infra::groq::{GroqError, TranscribeParams};
use crate::infra::sse::SsePayload;
use crate::repositories;
use crate::state::AppState;

/// Jalankan satu job sampai final. Semua kegagalan di-map ke status DB yang benar.
pub async fn run_job(state: &AppState, job: &TranscriptRow) {
    let started = OffsetDateTime::now_utc();
    tracing::info!(transcript_id = %job.id, "job dimulai");

    let result = run_inner(state, job).await;
    match result {
        Ok(()) => {
            tracing::info!(
                transcript_id = %job.id,
                total_ms = (OffsetDateTime::now_utc() - started).whole_milliseconds(),
                "job selesai"
            );
        }
        Err(e) => {
            tracing::error!(transcript_id = %job.id, error = %e, "job gagal");
            let msg = e.friendly();
            let _ = repositories::transcripts::fail(&state.db, job.id, &msg).await;
            state
                .sse
                .publish(job.id, SsePayload::Failed { message: msg });
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum JobError {
    #[error("{0}")]
    Fatal(String),
    #[error("groq: {0}")]
    Groq(#[from] GroqError),
}

impl JobError {
    fn friendly(&self) -> String {
        match self {
            JobError::Fatal(m) => m.clone(),
            JobError::Groq(e) => e.friendly(),
        }
    }
}

async fn run_inner(state: &AppState, job: &TranscriptRow) -> Result<(), JobError> {
    // 1) Unduh audio dari storage ke temp.
    let temp = tempfile::tempdir().map_err(|e| JobError::Fatal(format!("temp dir: {e}")))?;
    let source = temp.path().join("source");
    state
        .storage
        .get_file(&job.storage_key, &source)
        .await
        .map_err(|e| JobError::Fatal(format!("audio tidak ditemukan di storage: {e}")))?;

    let size = tokio::fs::metadata(&source)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    let needs_chunking = size > state.cfg.groq_chunk_bytes as u64;

    // Retry cleanup: job gagal bisa punya segments parsial dari percobaan lama.
    repositories::transcripts::delete_segments(&state.db, job.id).await?;

    // 2) Siapkan chunk: file besar dipotong via ffmpeg (tanpa re-encode).
    let chunks: Vec<std::path::PathBuf> = if needs_chunking {
        transition(state, job.id, "validating", "chunking").await;
        state.sse.publish(
            job.id,
            SsePayload::Progress {
                status: "chunking".into(),
                progress: 2,
                message: Some("memotong audio".into()),
            },
        );
        infra::ffmpeg::split(
            &state.cfg.ffmpeg_path,
            &source,
            state.cfg.groq_chunk_seconds,
            temp.path(),
            &job.mime_type,
        )
        .await
        .map_err(|e| JobError::Fatal(format!("gagal memotong audio: {e}")))?
    } else {
        vec![source]
    };

    transition(state, job.id, "validating", "transcribing").await;

    // 3) Transkripsi per chunk dengan offset timestamp & checkpoint cancel.
    let mut detected_language: Option<String> = None;
    let mut language_probability: Option<f32> = None;
    let mut running_prompt: Option<String> = job.prompt_hint.clone().filter(|p| !p.is_empty());
    let mut base_idx: i32 = 0;
    let mut offset_ms: i64 = 0;

    for (i, chunk) in chunks.iter().enumerate() {
        // Checkpoint cancel antar chunk (docs/02 §4).
        if repositories::transcripts::is_cancel_requested(&state.db, job.id).await? {
            return cancel_job(state, job).await;
        }

        let chunk_duration_ms = probe_ms(state, chunk).await;
        let bytes = tokio::fs::read(chunk)
            .await
            .map_err(|e| JobError::Fatal(format!("baca chunk: {e}")))?;

        let params = TranscribeParams {
            model: &job.model,
            language: job.requested_language.as_deref(),
            prompt: running_prompt.as_deref(),
            filename: &chunk_file_name(chunk, &job.source_filename, i, &job.mime_type),
            mime: &infra::groq::groq_mime(&job.mime_type),
        };
        let result = state.groq.transcribe(bytes, &params).await?;

        if detected_language.is_none() {
            detected_language = result.language.clone();
            language_probability = result.language_probability.map(|p| p as f32);
        }

        // Simpan segments + geser timestamp (docs/02 §4 offset).
        let segs: Vec<repositories::transcripts::SegIn> = result
            .segments
            .iter()
            .map(|s| repositories::transcripts::SegIn {
                start_ms: (s.start * 1000.0) as i64,
                end_ms: (s.end * 1000.0) as i64,
                text: s.text.trim().to_string(),
                confidence: s.no_speech_prob.map(|p| (1.0 - p) as f32),
            })
            .collect();
        repositories::transcripts::insert_segments(&state.db, job.id, base_idx, offset_ms, &segs)
            .await?;
        base_idx += result.segments.len() as i32;
        offset_ms += chunk_duration_ms;

        // Kontinuitas antar chunk: bawa ekor teks sebagai prompt berikutnya.
        running_prompt = build_next_prompt(running_prompt.as_deref(), &result.text);

        let progress = (((i + 1) * 100) / chunks.len()).min(99) as i16;
        repositories::transcripts::set_progress(&state.db, job.id, "transcribing", progress)
            .await?;
        state.sse.publish(
            job.id,
            SsePayload::Progress {
                status: "transcribing".into(),
                progress,
                message: Some(format!("chunk {}/{}", i + 1, chunks.len())),
            },
        );
    }

    // 4) Finalisasi.
    repositories::transcripts::set_progress(&state.db, job.id, "processing", 99).await?;
    repositories::transcripts::finalize_completed(
        &state.db,
        job.id,
        detected_language.as_deref(),
        language_probability,
    )
    .await?;
    let _ = repositories::transcripts::log_event(
        &state.db,
        job.id,
        "transcribing",
        "completed",
        &format!(
            "selesai: {} segment",
            repositories::transcripts::segments_for(&state.db, job.id)
                .await
                .map(|s| s.len())
                .unwrap_or(0)
        ),
    )
    .await;

    let duration = job.duration_seconds;
    state.sse.publish(
        job.id,
        SsePayload::Completed {
            transcript_id: job.id,
            duration_seconds: duration,
            language: detected_language,
        },
    );
    Ok(())
}

async fn cancel_job(state: &AppState, job: &TranscriptRow) -> Result<(), JobError> {
    repositories::transcripts::mark_cancelled(&state.db, job.id).await?;
    let _ = repositories::transcripts::log_event(
        &state.db,
        job.id,
        "transcribing",
        "cancelled",
        "dibatalkan user",
    )
    .await;
    // Audio ditandai hapus (docs/02 §4).
    let _ = state.storage.delete(&job.storage_key).await;
    let _ = repositories::transcripts::mark_audio_purged(&state.db, job.id).await;
    state.sse.publish(job.id, SsePayload::Cancelled);
    tracing::info!(transcript_id = %job.id, "job dibatalkan");
    Ok(())
}

async fn transition(state: &AppState, id: Uuid, from: &str, to: &str) {
    let ok = repositories::transcripts::set_status(&state.db, id, from, to)
        .await
        .unwrap_or(false);
    if !ok {
        tracing::warn!(transcript_id = %id, from, to, "transisi state dilewati (status berubah)");
    }
}

async fn probe_ms(state: &AppState, path: &Path) -> i64 {
    infra::ffmpeg::probe(&state.cfg.ffprobe_path, path)
        .await
        .ok()
        .and_then(|p| p.duration_seconds)
        .map(|d| (d * 1000.0) as i64)
        .unwrap_or(0)
}

fn chunk_file_name(chunk: &Path, original: &str, idx: usize, mime: &str) -> String {
    // Ekstensi dari MIME (bukan path) — file temp single-chunk tidak berekstensi.
    let ext = infra::ffmpeg::ext_for_mime(mime).unwrap_or("bin");
    let stem = std::path::Path::new(original)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("audio");
    let _ = chunk;
    format!("{stem}-{idx:03}.{ext}")
}

/// Prompt berikutnya = prompt user + 300 karakter terakhir teks chunk sebelumnya.
fn build_next_prompt(user_prompt: Option<&str>, last_text: &str) -> Option<String> {
    let tail: String = last_text
        .chars()
        .rev()
        .take(300)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    match user_prompt {
        Some(p) if !tail.is_empty() => Some(format!("{p}\n{tail}")),
        Some(p) => Some(p.to_string()),
        None if !tail.is_empty() => Some(tail),
        None => None,
    }
}

impl From<sqlx::Error> for JobError {
    fn from(e: sqlx::Error) -> Self {
        JobError::Fatal(format!("db: {e}"))
    }
}
