//! Supervisor worker — N task konsumen antrean + reaper + retensi (docs/02 §5, §7).

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::repositories;
use crate::state::AppState;
use crate::workers::transcribe;

const STALE_MINUTES: i64 = 10;
const REAPER_INTERVAL: Duration = Duration::from_secs(60);
const RETENTION_INTERVAL: Duration = Duration::from_secs(3600);

/// Spawn semua task worker. Dipanggil dari main; mati mengikuti runtime.
pub async fn run(state: AppState) {
    if state.cfg.groq_api_key.is_none() {
        tracing::warn!(
            "GROQ_API_KEY kosong — worker transkripsi TIDAK dijalankan (upload tetap jalan)"
        );
        return;
    }

    let active: Arc<Mutex<HashSet<uuid::Uuid>>> = Arc::new(Mutex::new(HashSet::new()));

    for n in 0..state.cfg.worker_concurrency.max(1) {
        let state = state.clone();
        let active = active.clone();
        tokio::spawn(async move {
            tracing::info!(worker = n, "worker mulai");
            worker_loop(state, active).await;
        });
    }

    tokio::spawn(reaper_loop(state.clone(), active));
    tokio::spawn(retention_loop(state));
}

async fn worker_loop(state: AppState, active: Arc<Mutex<HashSet<uuid::Uuid>>>) {
    loop {
        let job = repositories::transcripts::claim_next_job(&state.db).await;
        match job {
            Ok(Some(job)) => {
                if let Ok(mut set) = active.lock() {
                    set.insert(job.id);
                }
                transcribe::run_job(&state, &job).await;
                if let Ok(mut set) = active.lock() {
                    set.remove(&job.id);
                }
            }
            Ok(None) => {
                tokio::time::sleep(Duration::from_millis(state.cfg.worker_poll_interval_ms)).await;
            }
            Err(e) => {
                tracing::error!(error = %e, "claim job gagal");
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
    }
}

/// Job stuck >10 menit (worker crash) → kembalikan ke antrean (docs/04 §5).
async fn reaper_loop(state: AppState, active: Arc<Mutex<HashSet<uuid::Uuid>>>) {
    loop {
        tokio::time::sleep(REAPER_INTERVAL).await;
        let exclude: Vec<uuid::Uuid> = active
            .lock()
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default();
        match repositories::transcripts::reset_stale_jobs(&state.db, STALE_MINUTES, &exclude).await
        {
            Ok(reset) if !reset.is_empty() => {
                tracing::warn!(count = reset.len(), "job stale di-reset ke antrean");
            }
            Err(e) => tracing::error!(error = %e, "reaper gagal"),
            _ => {}
        }
    }
}

/// Hapus audio yang sudah melewati retensi (docs/04 §4).
async fn retention_loop(state: AppState) {
    loop {
        tokio::time::sleep(RETENTION_INTERVAL).await;
        match repositories::transcripts::expired_audio_keys(
            &state.db,
            state.cfg.audio_retention_days,
        )
        .await
        {
            Ok(keys) => {
                for (id, key) in keys {
                    match state.storage.delete(&key).await {
                        Ok(()) => {
                            let _ =
                                repositories::transcripts::mark_audio_purged(&state.db, id).await;
                            tracing::info!(transcript_id = %id, "audio kedaluwarsa dihapus");
                        }
                        Err(e) => {
                            tracing::warn!(transcript_id = %id, error = %e, "gagal hapus audio")
                        }
                    }
                }
            }
            Err(e) => tracing::error!(error = %e, "retention check gagal"),
        }
    }
}
