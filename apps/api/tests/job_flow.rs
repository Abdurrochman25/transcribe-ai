//! Integration test pipeline transkripsi (fase 3) — Groq di-mock via wiremock.
//! Butuh: Postgres lokal (DATABASE_URL) + ffprobe/ffmpeg di PATH.

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use transcribe_api::{
    app,
    config::{Config, LogFormat},
    repositories,
    state::AppState,
    workers::transcribe as worker,
};

// ===== Harness =====

fn test_config(groq_base: String) -> Config {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_env_filter("transcribe_api=debug,sqlx=warn")
        .try_init();
    Config {
        port: 0,
        database_url: String::new(),
        jwt_secret: "test-secret-32-byte-integration-aaaa!".into(),
        groq_api_key: Some("test-key".into()),
        groq_base_url: groq_base,
        cors_allowed_origins: vec![],
        log_format: LogFormat::Text,
        access_token_ttl_secs: 900,
        refresh_token_ttl_secs: 2_592_000,
        cookie_secure: false,
        storage_driver: "local".into(),
        s3_endpoint: None,
        s3_bucket: None,
        s3_access_key: None,
        s3_secret_key: None,
        s3_region: None,
        storage_local_root: String::new(),
        max_upload_bytes: 1024 * 1024,
        ffprobe_path: "ffprobe".into(),
        ffmpeg_path: "ffmpeg".into(),
        groq_max_retries: 3,
        groq_retry_base_ms: 1,
        groq_chunk_bytes: 1024 * 1024,
        groq_chunk_seconds: 600,
        worker_concurrency: 1,
        worker_poll_interval_ms: 50,
        audio_retention_days: 30,
    }
}

fn ip() -> String {
    format!("10.{}.{}.{}", rnd(), rnd(), rnd() + 1)
}

fn rnd() -> u32 {
    (uuid::Uuid::now_v7().to_u128_le() % 250) as u32
}

fn verbose_json() -> Value {
    json!({
        "text": "Selamat pagi, hari ini kita belajar. Materi tentang RAG dan LLM.",
        "language": "id",
        "language_probability": 0.98,
        "duration": 0.1,
        "segments": [
            { "id": 0, "start": 0.0, "end": 0.05, "text": "Selamat pagi, hari ini kita belajar.", "no_speech_prob": 0.02 },
            { "id": 1, "start": 0.05, "end": 0.1, "text": "Materi tentang RAG dan LLM.", "no_speech_prob": 0.01 }
        ]
    })
}

async fn register(router: &axum::Router) -> String {
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-forwarded-for", ip())
                .body(Body::from(
                    json!({ "email": format!("{}@t.id", uuid::Uuid::now_v7()), "password": "rahasia-aman-2026", "name": "T" })
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice::<Value>(&bytes).unwrap()["tokens"]["access_token"]
        .as_str()
        .unwrap()
        .to_string()
}

/// WAV mono 8kHz.
fn wav_bytes(samples: usize) -> Vec<u8> {
    let data_len = (samples * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&8000u32.to_le_bytes());
    b.extend_from_slice(&16000u32.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..samples {
        b.extend_from_slice(&(((i % 97) as i16 - 48) * 200).to_le_bytes());
    }
    b
}

async fn upload(router: &axum::Router, token: &str, filename: &str, bytes: &[u8]) -> uuid::Uuid {
    let boundary = "----jobtest";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/transcripts")
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header("x-forwarded-for", ip())
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);
    let b = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: Value = serde_json::from_slice(&b).unwrap();
    uuid::Uuid::parse_str(v["transcript"]["id"].as_str().unwrap()).unwrap()
}

async fn get_detail(router: &axum::Router, token: &str, id: uuid::Uuid) -> Value {
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/transcripts/{id}"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let b = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&b).unwrap()
}

async fn claim(pool: &PgPool) -> transcribe_api::domain::transcript::TranscriptRow {
    repositories::transcripts::claim_next_job(pool)
        .await
        .unwrap()
        .expect("ada job queued (baru diupload)")
}

async fn mount_ok(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(verbose_json()))
        .mount(server)
        .await;
} // ===== Tests =====

#[sqlx::test(migrations = "./migrations")]
async fn happy_path_single_chunk(pool: PgPool) {
    let server = MockServer::start().await;
    mount_ok(&server).await;

    let state = AppState::for_test(pool.clone(), test_config(server.uri()));
    let router = app(state.clone());
    let token = register(&router).await;
    let id = upload(&router, &token, "kuliah.wav", &wav_bytes(800)).await;

    let job = claim(&pool).await;
    assert_eq!(job.id, id);

    worker::run_job(&state, &job).await;

    let detail = get_detail(&router, &token, id).await;
    assert_eq!(detail["transcript"]["status"], "completed");
    assert_eq!(detail["transcript"]["progress"], 100);
    assert_eq!(detail["transcript"]["language"], "id");
    let segs = detail["segments"].as_array().unwrap();
    assert_eq!(segs.len(), 2, "2 segment dari mock: {segs:?}");
    assert_eq!(segs[0]["text"], "Selamat pagi, hari ini kita belajar.");
    assert!(segs[0]["start_ms"].as_i64().unwrap() < segs[1]["start_ms"].as_i64().unwrap());
}

#[sqlx::test(migrations = "./migrations")]
async fn retryable_429_lalu_sukses(pool: PgPool) {
    let server = MockServer::start().await;
    // Mock dicocokkan LIFO: 200 dipasang dulu, lalu 429 (2x) → dua pertama 429, ketiga 200.
    Mock::given(method("POST"))
        .and(path("/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(verbose_json()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/audio/transcriptions"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "0")
                .set_body_string("{\"error\":{\"message\":\"rate limited\"}}"),
        )
        .up_to_n_times(2)
        .mount(&server)
        .await;

    let state = AppState::for_test(pool.clone(), test_config(server.uri()));
    let router = app(state.clone());
    let token = register(&router).await;
    let id = upload(&router, &token, "a.wav", &wav_bytes(800)).await;
    let job = claim(&pool).await;

    worker::run_job(&state, &job).await;

    let detail = get_detail(&router, &token, id).await;
    assert_eq!(
        detail["transcript"]["status"], "completed",
        "harus lolos setelah retry 429"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn non_retryable_400_job_gagal_lalu_retry(pool: PgPool) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/audio/transcriptions"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_string("{\"error\":{\"message\":\"invalid file\"}}"),
        )
        .mount(&server)
        .await;

    let state = AppState::for_test(pool.clone(), test_config(server.uri()));
    let router = app(state.clone());
    let token = register(&router).await;
    let id = upload(&router, &token, "a.wav", &wav_bytes(800)).await;
    let job = claim(&pool).await;

    worker::run_job(&state, &job).await;

    let detail = get_detail(&router, &token, id).await;
    assert_eq!(detail["transcript"]["status"], "failed");
    assert!(
        detail["transcript"]["error"]
            .as_str()
            .unwrap_or("")
            .contains("invalid file"),
        "pesan error tersimpan"
    );

    // POST retry → status queued kembali (docs/03).
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/transcripts/{id}/retry"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[sqlx::test(migrations = "./migrations")]
async fn cancel_checkpoint_menghentikan_job(pool: PgPool) {
    let server = MockServer::start().await;
    mount_ok(&server).await;

    let state = AppState::for_test(pool.clone(), test_config(server.uri()));
    let router = app(state.clone());
    let token = register(&router).await;
    let id = upload(&router, &token, "a.wav", &wav_bytes(800)).await;
    let job = claim(&pool).await;

    // User minta cancel SEBELUM worker jalan.
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/transcripts/{id}/cancel"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);

    worker::run_job(&state, &job).await;

    let detail = get_detail(&router, &token, id).await;
    assert_eq!(detail["transcript"]["status"], "cancelled");
    assert_eq!(detail["segments"].as_array().unwrap().len(), 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn chunking_multi_chunk_dengan_offset(pool: PgPool) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(verbose_json()))
        .expect(2..)
        .mount(&server)
        .await;

    // Chunking agresif: file 2.5 detik dipotong per 1 detik → 3 chunk.
    let mut cfg = test_config(server.uri());
    cfg.groq_chunk_bytes = 1;
    cfg.groq_chunk_seconds = 1;

    let state = AppState::for_test(pool.clone(), cfg);
    let router = app(state.clone());

    let token = register(&router).await;
    let id = upload(&router, &token, "panjang.wav", &wav_bytes(20000)).await; // 2.5 detik
    let job = claim(&pool).await;

    worker::run_job(&state, &job).await;

    let detail = get_detail(&router, &token, id).await;
    assert_eq!(detail["transcript"]["status"], "completed");
    let segs = detail["segments"].as_array().unwrap();
    assert!(segs.len() >= 4, "≥2 chunk × 2 segment: {segs:?}");

    // Offset timestamp: segment pertama chunk-2 mulai ≥ durasi chunk-1 (1s, toleransi ffmpeg).
    let starts: Vec<i64> = segs
        .iter()
        .map(|s| s["start_ms"].as_i64().unwrap())
        .collect();
    let mut sorted = starts.clone();
    sorted.sort();
    assert_eq!(starts, sorted, "start_ms monoton naik");
    assert!(
        segs[2]["start_ms"].as_i64().unwrap() >= 900,
        "offset chunk diterapkan: {:?}",
        starts
    );
}
