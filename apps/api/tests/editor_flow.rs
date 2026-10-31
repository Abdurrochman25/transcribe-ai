//! Integration test editor & export (fase 4) — PATCH, export, audio Range.
//! Butuh: Postgres lokal (DATABASE_URL).

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

use transcribe_api::{
    app,
    config::{Config, LogFormat},
    state::AppState,
};

// ===== Harness (pola sama dengan upload_flow/job_flow) =====

fn test_config() -> Config {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_env_filter("transcribe_api=warn,sqlx=warn")
        .try_init();
    Config {
        port: 0,
        database_url: String::new(),
        jwt_secret: "test-secret-32-byte-integration-aaaa!".into(),
        groq_api_key: Some("test-key".into()),
        groq_base_url: "http://127.0.0.1:9".into(), // tidak dipakai di test ini
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

/// WAV mono 8kHz — `samples` sampel = durasi sama dalam ms/8.
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

async fn upload(router: &axum::Router, token: &str, bytes: &[u8]) -> uuid::Uuid {
    let boundary = "----editortest";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.wav\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes(),
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

/// Paksa transcript jadi completed + isi 2 segment (tanpa pipeline penuh).
async fn force_completed(pool: &PgPool, id: uuid::Uuid) {
    sqlx::query(
        "UPDATE transcripts SET status='completed', progress=100, completed_at=now() WHERE id=$1",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    for (idx, (start, end, text)) in [
        (0, 4210i64, "Selamat pagi Bapak/Ibu."),
        (4210, 9870, "Hari ini kita bahas LLM."),
    ]
    .into_iter()
    .enumerate()
    {
        sqlx::query("INSERT INTO segments (transcript_id, idx, start_ms, end_ms, text) VALUES ($1,$2,$3,$4,$5)")
            .bind(id)
            .bind(idx as i32)
            .bind(start)
            .bind(end)
            .bind(text)
            .execute(pool)
            .await
            .unwrap();
    }
}

async fn request_json(
    router: &axum::Router,
    method: &str,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> (StatusCode, Option<Value>, header::HeaderMap) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"));
    let body = match body {
        Some(v) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let res = router
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = if bytes.is_empty() {
        None
    } else {
        serde_json::from_slice(&bytes).ok()
    };
    (status, json, headers)
}

// ===== Tests =====

#[sqlx::test(migrations = "./migrations")]
async fn patch_judul_dan_segment(pool: PgPool) {
    let state = AppState::for_test(pool.clone(), test_config());
    let router = app(state);
    let token = register(&router).await;
    let id = upload(&router, &token, &wav_bytes(800)).await;
    force_completed(&pool, id).await;

    let (status, body, _) = request_json(
        &router,
        "PATCH",
        &format!("/api/v1/transcripts/{id}"),
        &token,
        Some(json!({
            "title": "Kuliah AI #3 — LLM",
            "segments": [ { "idx": 1, "text": "  Hari ini kita bahas LLM dan RAG.  " } ]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");

    let t = body.as_ref().unwrap()["transcript"].clone();
    assert_eq!(t["title"], "Kuliah AI #3 — LLM");
    let segs = body.as_ref().unwrap()["segments"].as_array().unwrap();
    assert_eq!(segs[0]["text"], "Selamat pagi Bapak/Ibu.");
    assert_eq!(segs[0]["edited"], false, "segment tak disentuh tetap false");
    assert_eq!(
        segs[1]["text"], "Hari ini kita bahas LLM dan RAG.",
        "trim + update"
    );
    assert_eq!(segs[1]["edited"], true, "jejak edit");

    // audio_url tersedia untuk player (docs/03).
    assert_eq!(
        body.as_ref().unwrap()["audio_url"],
        json!(format!("/api/v1/transcripts/{id}/audio"))
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_segment_ditolak_sebelum_completed(pool: PgPool) {
    let state = AppState::for_test(pool.clone(), test_config());
    let router = app(state);
    let token = register(&router).await;
    let id = upload(&router, &token, &wav_bytes(800)).await; // masih queued

    let (status, _, _) = request_json(
        &router,
        "PATCH",
        &format!("/api/v1/transcripts/{id}"),
        &token,
        Some(json!({ "segments": [ { "idx": 0, "text": "x" } ] })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_judul_invalid(pool: PgPool) {
    let state = AppState::for_test(pool.clone(), test_config());
    let router = app(state);
    let token = register(&router).await;
    let id = upload(&router, &token, &wav_bytes(800)).await;

    let (status, body, _) = request_json(
        &router,
        "PATCH",
        &format!("/api/v1/transcripts/{id}"),
        &token,
        Some(json!({ "title": "   " })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body.unwrap()["error"]["code"], "VALIDATION_ERROR");
}

#[sqlx::test(migrations = "./migrations")]
async fn export_txt_srt_vtt_json(pool: PgPool) {
    let state = AppState::for_test(pool.clone(), test_config());
    let router = app(state);
    let token = register(&router).await;
    let id = upload(&router, &token, &wav_bytes(800)).await;
    force_completed(&pool, id).await;

    // SRT
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/transcripts/{id}/export?format=srt"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()[header::CONTENT_TYPE], "application/x-subrip");
    let disp = res.headers()[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .to_string();
    assert!(disp.starts_with("attachment; filename=\"a.srt\""), "{disp}");
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let srt = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(
        srt.contains("00:00:00,000 --> 00:00:04,210\nSelamat pagi Bapak/Ibu."),
        "{srt}"
    );
    assert!(srt.contains("00:00:04,210 --> 00:00:09,870"));

    // VTT
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/transcripts/{id}/export?format=vtt"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let vtt = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(vtt.starts_with("WEBVTT\n\n"));
    assert!(vtt.contains("00:00:04.210 --> 00:00:09.870"));

    // JSON valid
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/transcripts/{id}/export?format=json"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["transcript"]["id"].as_str().unwrap(), id.to_string());
    assert_eq!(v["segments"].as_array().unwrap().len(), 2);

    // Format tak dikenal → 400
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/transcripts/{id}/export?format=mp4"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn audio_stream_range_dan_kepemilikan(pool: PgPool) {
    let state = AppState::for_test(pool.clone(), test_config());
    let router = app(state);
    let token = register(&router).await;
    let bytes_wav = wav_bytes(800);
    let id = upload(&router, &token, &bytes_wav).await;
    force_completed(&pool, id).await;

    // Tanpa Range → 200 penuh + Accept-Ranges.
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/transcripts/{id}/audio?token={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()[header::CONTENT_TYPE], "audio/x-wav");
    assert_eq!(res.headers()[header::ACCEPT_RANGES], "bytes");
    assert_eq!(
        res.headers()[header::CONTENT_LENGTH],
        bytes_wav.len().to_string()
    );

    // Range 0-99 → 206 + prefix cocok.
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/transcripts/{id}/audio?token={token}"))
                .header(header::RANGE, "bytes=0-99")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(res.headers()[header::CONTENT_LENGTH], "100");
    let got = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&got[..], &bytes_wav[..100]);

    // Token tidak valid → 401.
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/transcripts/{id}/audio?token=sampah"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // User lain → 404 (anti enumerasi).
    let token_b = register(&router).await;
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/transcripts/{id}/audio"))
                .header(header::AUTHORIZATION, format!("Bearer {token_b}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
