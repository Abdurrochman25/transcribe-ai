# 05 — Backend Design (Rust / Axum)

## 1. Crate & Dependensi

Single crate `apps/api` (binary), edisi Rust 2024, modular monolith (lihat [ADR-005](adr/005-modular-monolith.md)).

```toml
[package]
name = "transcribe-api"
version = "0.1.0"
edition = "2024"

[dependencies]
# web
axum = { version = "0.8", features = ["multipart", "macros"] }
tokio = { version = "1", features = ["full"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["trace", "cors", "compression-gzip", "timeout", "limit"] }

# data
sqlx = { version = "0.8", features = ["postgres", "runtime-tokio", "uuid", "time", "rust_decimal", "migrate"] }
uuid = { version = "1", features = ["v7", "serde"] }
time = { version = "0.3", features = ["serde-well-known"] }

# serialization & validation
serde = { version = "1", features = ["derive"] }
serde_json = "1"
validator = { version = "0.20", features = ["derive"] }

# auth & security
argon2 = "0.5"
jsonwebtoken = "10"
rand = "0.9"

# http client (Groq)
reqwest = { version = "0.12", features = ["multipart", "stream", "json"] }

# observability & errors
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
thiserror = "2"
anyhow = "1"

# infra
object_store = { version = "0.12", features = ["aws", "http"] }  # S3/MinIO/R2
dashmap = "6"
futures = "0.3"
tokio-util = { version = "0.7", features = ["io"] }
governor = "0.10"          # rate limiting
sha2 = "0.10"              # hash refresh token / api key
base64 = "0.22"

[dev-dependencies]
sqlx-test = "*"
pretty_assertions = "1"
wiremock = "0.6"            # mock Groq
```

## 2. Struktur Modul

```
apps/api/src/
├── main.rs               # bootstrap: config → tracing → pool → router → serve
├── lib.rs                # agar bisa di-integration-test
├── config.rs             # env-driven, serde, fail-fast
├── error.rs              # AppError → (status, code, message); From<sqlx::Error> dll.
├── state.rs              # AppState { db, storage, groq, sse, cfg }
├── routes/
│   ├── mod.rs            # Router v1 + middleware layers
│   ├── auth.rs
│   ├── transcripts.rs
│   └── misc.rs           # health/ready/languages
├── middleware/
│   ├── auth.rs           # extractor AuthUser (JWT → user_id)
│   └── rate_limit.rs     # governor keyed user/IP
├── services/
│   ├── auth.rs           # register/login/refresh-rotation/logout
│   ├── transcript.rs     # upload orchestration, list/get/patch
│   └── export.rs         # txt/srt/vtt/json renderer
├── repositories/
│   ├── users.rs
│   ├── tokens.rs
│   └── transcripts.rs    # claim_job (SKIP LOCKED), save_segments, ...
├── domain/
│   ├── mod.rs
│   ├── user.rs
│   ├── transcript.rs     # entity + TranscriptStatus + state machine
│   └── events.rs         # SseEvent payload
├── workers/
│   ├── mod.rs            # supervisor N task + reaper
│   └── transcribe.rs     # pipeline per job
├── infra/
│   ├── groq.rs           # GroqClient + retry
│   ├── storage.rs        # ObjectStore wrapper + presign
│   ├── ffmpeg.rs         # ffprobe/ffmpeg subprocess wrapper
│   └── sse.rs            # SseHub
└── util/                 # time format, magic-bytes, dsb.
```

Aturan dependensi antar layer (dicek di review/CI via `cargo modules` atau kesepakatan):
`routes → services → repositories/domain`, `services → infra`, `domain` tidak bergantung siapa pun.

## 3. Config (env)

| Var | Default | Keterangan |
|---|---|---|
| `PORT` | `8080` | |
| `DATABASE_URL` | — (wajib) | postgres:// |
| `S3_ENDPOINT` / `S3_BUCKET` / `S3_ACCESS_KEY` / `S3_SECRET_KEY` / `S3_REGION` | — | MinIO/S3/R2 |
| `GROQ_API_KEY` | — (wajib) | |
| `GROQ_BASE_URL` | `https://api.groq.com/openai/v1` | bisa diarahkan ke mock di test |
| `JWT_SECRET` | — (wajib, ≥32 byte) | HS256 |
| `ACCESS_TOKEN_TTL_SECS` | `900` | 15 menit |
| `REFRESH_TOKEN_TTL_SECS` | `2592000` | 30 hari |
| `MAX_UPLOAD_BYTES` | `209715200` | 200 MB |
| `GROQ_CHUNK_BYTES` | `26214400` | 25 MB — batas aman |
| `GROQ_CHUNK_SECONDS` | `600` | chunk 10 menit |
| `WORKER_CONCURRENCY` | `4` | |
| `WORKER_POLL_INTERVAL_MS` | `2000` | |
| `AUDIO_RETENTION_DAYS` | `30` | |
| `CORS_ALLOWED_ORIGINS` | `http://localhost:5173` | koma-separated |
| `RUST_LOG` | `info,transcribe_api=debug` | |

## 4. Inti: Worker Pipeline Transkripsi

```rust
// workers/transcribe.rs (sketsa)
pub async fn run_job(ctx: JobCtx) -> Result<()> {
    let audio = ctx.storage.get_stream(&ctx.job.storage_key).await?;
    let meta = ffmpeg::probe(&audio).await
        .map_err(|e| ctx.fail(format!("File tidak bisa dibaca: {e}")))?;   // validating

    let chunks = if meta.size_bytes > ctx.cfg.groq_chunk_bytes {
        ffmpeg::split(&audio, ctx.cfg.groq_chunk_seconds).await?            // chunking
    } else { vec![audio] };

    let mut t_offset = 0u64; let mut seg_idx = 0usize; ctx.set_status(Transcribing);
    for (i, chunk) in chunks.iter().enumerate() {
        ctx.check_cancelled().await?;                                        // checkpoint cancel
        let res = ctx.groq.transcribe(TranscribeReq {                        // retry di dalam
            file: chunk.clone(), model: ctx.job.model.clone(),
            language: ctx.job.requested_language.clone(),
            prompt: ctx.job.prompt_hint.clone(),
            response_format: "verbose_json",
        }).await?;
        ctx.repo.save_segments(&ctx.job.id, res.segments, t_offset, seg_idx).await?;
        seg_idx += res.segments.len(); t_offset += chunk.duration_ms;
        ctx.publish_progress((i + 1) * 100 / chunks.len(), format!("chunk {}/{chunks.len()}")).await?;
    }

    ctx.repo.finalize(&ctx.job.id, meta.duration, res.language, res.language_prob).await?;
    ctx.publish(Completed).await                                             // completed
}
```

Poin penting:
- **Checkpoint cancel** antar chunk; cancel → `status=cancelled`, S3 object ditandai hapus.
- **Offset timestamp**: `start_ms`/`end_ms` tiap segment chunk ke-n digeser `sum(durasi chunk < n)`.
- **Overlap minimal**: chunk dipotong pada batas silence ketika memungkinkan (`ffmpeg silencedetect`) — roadmap v1.1; MVP potong lurus (Whisper konteks per-chunk sudah cukup baik).

## 5. Infra: GroqClient

```rust
pub struct GroqClient { http: reqwest::Client, base: Url, key: String }

pub struct TranscribeReq<'a> { file: NamedBytes, model: &'a str, language: Option<String>,
                               prompt: Option<String>, response_format: &'a str }

impl GroqClient {
    /// Retry: 429/500/502/503/504 → backoff eksponensial + jitter, max 3.
    /// Hormati header Retry-After bila ada.
    pub async fn transcribe(&self, req: TranscribeReq<'_>) -> Result<GroqVerbose, GroqError> { ... }
}
```

Respons `verbose_json` yang dipetakan (field yang dipakai):
`text`, `language`, `language_probability`, `duration`, `segments[] { id, start, end, text, no_speech_prob, avg_logprob }`.
- `confidence` disimpan sebagai `1 - no_speech_prob`-ish heuristik (opsional; boleh null).
- Error 400 `invalid_file` → non-retryable (job `failed` + pesan jelas); sisanya dicek retryable.

## 6. Error Handling

```rust
#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("unauthenticated")] Unauthenticated,
    #[error("forbidden")] Forbidden,
    #[error("not found")] NotFound,
    #[error("conflict: {0}")] Conflict(&'static str),
    #[error("validation: {0}")] Validation(Vec<FieldError>),
    #[error("payload too large")] TooLarge,
    #[error("rate limited")] RateLimited,
    #[error(transparent)] Db(#[from] sqlx::Error),
    #[error(transparent)] Storage(#[from] object_store::Error),
    #[error("internal")] Internal(#[from] anyhow::Error),
}
// impl IntoResponse → format error API spec 03
```
Semua error ter-log dengan `request_id` (middleware) → trace-able.

## 7. Testing Strategy

| Level | Tool | Cakupan |
|---|---|---|
| Unit `domain/` | `cargo test` | state machine transisi valid/invalid, formatter SRT/VTT, offset timestamp |
| Integration repo | `sqlx::test` (DB ephemeral per test) | claim job race (2 task concurrent → beda job), save_segments |
| Integration API | `axum::serve` + `reqwest` in-proc + `wiremock` (Groq) | upload→202→mock chunk→completed; SSE event stream; 401/403/409 |
| E2E (nanti) | docker-compose + Playwright (repo frontend) | happy path penuh |

CI menjalankan `fmt --check`, `clippy -D warnings`, `test` (lihat 09-deployment).

## 8. Observability

- `tracing` JSON log + `request_id` (UUIDv7) di semua span; `TraceLayer`.
- Log penting: `job.claimed`, `job.chunk_done {i,n,ms}`, `job.completed {total_ms}`, `groq.retry {attempt}`, `groq.error {status}`.
- `/ready` memeriksa DB (`SELECT 1`) & storage (HEAD bucket).
- Metrics Prometheus (opsional v1.1): job queue depth, job duration histogram, Groq error rate.

## 9. Siklus Hidup Proses

- Graceful shutdown: SIGTERM → stop accept → beri worker 30 detik menyelesaikan chunk → job belum selesai dibiarkan `status` non-final → reaper reset ke `queued` saat restart.
- Startup: `sqlx migrate` otomatis → verifikasi koneksi S3 & Groq (opsional, warning saja) → spawn worker supervisor + reaper + maintenance task (retensi).
