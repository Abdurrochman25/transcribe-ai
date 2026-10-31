//! Integration test upload transcript (fase 2).
//! Butuh: Postgres lokal + ffprobe di PATH.

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

// ===== Harness =====

fn test_config() -> Config {
    dotenvy::dotenv().ok();
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_env_filter("transcribe_api=debug,sqlx=warn")
        .try_init();
    Config {
        port: 0,
        database_url: String::new(),
        jwt_secret: "test-secret-32-byte-integration-aaaa!".into(),
        groq_api_key: None,
        groq_base_url: String::new(),
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
        max_upload_bytes: 1024 * 1024, // 1 MB — cukup untuk WAV test
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

fn ip_key() -> String {
    format!("10.{}.{}.{}", rand_byte(), rand_byte(), rand_byte() + 1)
}

fn rand_byte() -> u32 {
    (uuid::Uuid::now_v7().to_u128_le() % 250) as u32
}

async fn build_app(pool: PgPool) -> axum::Router {
    app(AppState::for_test(pool, test_config()))
}

/// Daftar user baru → (access_token, ip_key).
async fn register_user(router: &axum::Router, email: &str) -> (String, String) {
    let ip = ip_key();
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-forwarded-for", &ip)
                .body(Body::from(
                    json!({ "email": email, "password": "rahasia-aman-2026", "name": "Uploader" })
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
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    (
        body["tokens"]["access_token"].as_str().unwrap().to_string(),
        ip,
    )
}

/// WAV mono 8kHz sepanjang ~0.1 detik — valid untuk ffprobe.
fn tiny_wav() -> Vec<u8> {
    let sample_rate: u32 = 8000;
    let samples: usize = 800; // 0.1 s
    let data_len = (samples * 2) as u32;

    let mut buf = Vec::with_capacity(44 + data_len as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_len).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // mono
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    buf.extend_from_slice(&2u16.to_le_bytes()); // block align
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());
    // silence + sedikit variasi supaya bukan nol semua
    for i in 0..samples {
        let v = ((i % 100) as i16 - 50) * 100;
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf
}

/// Body multipart/form-data sederhana (satu file + field opsi).
fn multipart_body(boundary: &str, filename: &str, file: &[u8], title: Option<&str>) -> Vec<u8> {
    let mut body = Vec::new();
    if let Some(t) = title {
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\n{t}\r\n"
            )
            .as_bytes(),
        );
    }
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(file);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

async fn upload(
    router: &axum::Router,
    token: &str,
    ip: &str,
    filename: &str,
    file: &[u8],
    title: Option<&str>,
) -> (StatusCode, Option<Value>) {
    let boundary = "----transcribetestboundary";
    let body = multipart_body(boundary, filename, file, title);
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
                .header("x-forwarded-for", ip)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = if bytes.is_empty() {
        None
    } else {
        Some(serde_json::from_slice(&bytes).unwrap())
    };
    (status, json)
}

async fn get_list(router: &axum::Router, token: &str, query: &str) -> Value {
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/transcripts{query}"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

// ===== Tests =====

#[sqlx::test(migrations = "./migrations")]
async fn upload_sukses_masuk_antrean(pool: PgPool) {
    let router = build_app(pool).await;
    let (token, ip) = register_user(&router, "up@example.com").await;

    let (status, body) = upload(
        &router,
        &token,
        &ip,
        "kuliah.wav",
        &tiny_wav(),
        Some("Kuliah AI #1"),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body:?}");

    let t = &body.as_ref().unwrap()["transcript"];
    assert_eq!(t["status"], "queued");
    assert_eq!(t["title"], "Kuliah AI #1");
    assert_eq!(t["mime_type"], "audio/x-wav");
    assert_eq!(t["model"], "whisper-large-v3-turbo", "model default");
    assert!(
        t["duration_seconds"].as_f64().unwrap() > 0.05,
        "durasi terbaca ffprobe: {:?}",
        t["duration_seconds"]
    );

    // Muncul di list milik user.
    let list = get_list(&router, &token, "").await;
    assert_eq!(list["total"], 1);
    assert_eq!(list["items"][0]["title"], "Kuliah AI #1");
    assert_eq!(list["items"][0]["source_filename"], "kuliah.wav");
}

#[sqlx::test(migrations = "./migrations")]
async fn upload_ditolak_bukan_media(pool: PgPool) {
    let router = build_app(pool).await;
    let (token, ip) = register_user(&router, "up2@example.com").await;

    // File teks (>64 byte) — magic bytes tidak dikenali.
    let teks =
        b"ini bukan file audio sama sekali, hanya teks biasa untuk pengujian validation".to_vec();
    let (status, body) = upload(&router, &token, &ip, "rahasia.txt", &teks, None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
    assert_eq!(body.as_ref().unwrap()["error"]["code"], "VALIDATION_ERROR");

    // Ekskalasi nama .wav tetap ditolak — isi diperiksa, bukan ekstensi (docs/08 §2).
    let (status, _) = upload(&router, &token, &ip, "paluan.wav", &teks, None).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "ekstensi .wav dengan isi teks harus ditolak"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn upload_wajib_login_dan_file(pool: PgPool) {
    let router = build_app(pool).await;

    // Tanpa token → 401.
    let boundary = "----b";
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
                .header("x-forwarded-for", "10.9.9.9")
                .body(Body::from(multipart_body(
                    boundary,
                    "a.wav",
                    &tiny_wav(),
                    None,
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Login tapi tanpa field file → 400.
    let (token, ip) = register_user(&router, "up3@example.com").await;
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/transcripts")
                .header(header::CONTENT_TYPE, "multipart/form-data; boundary=----b")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header("x-forwarded-for", &ip)
                .body(Body::from("------b--\r\n".to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_dan_isolasi_antar_user(pool: PgPool) {
    let router = build_app(pool).await;
    let (token_a, ip_a) = register_user(&router, "a@example.com").await;
    let (token_b, _ip_b) = register_user(&router, "b@example.com").await;

    upload(
        &router,
        &token_a,
        &ip_a,
        "meeting.wav",
        &tiny_wav(),
        Some("Meeting sprint"),
    )
    .await;
    upload(
        &router,
        &token_a,
        &ip_a,
        "interview.wav",
        &tiny_wav(),
        Some("Interview CEO"),
    )
    .await;

    // User B tidak melihat milik A.
    let list_b = get_list(&router, &token_b, "").await;
    assert_eq!(list_b["total"], 0, "isolasi antar user");

    // Pencarian judul.
    let list_q = get_list(&router, &token_a, "?q=interview").await;
    assert_eq!(list_q["total"], 1);
    assert_eq!(list_q["items"][0]["title"], "Interview CEO");

    // Filter status valid; status ngawur → 400.
    let list_ok = get_list(&router, &token_a, "?status=queued").await;
    assert_eq!(list_ok["total"], 2);
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/transcripts?status=haram")
                .header(header::AUTHORIZATION, format!("Bearer {token_a}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // Pagination.
    let page = get_list(&router, &token_a, "?page=1&per_page=1").await;
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["total"], 2);
    assert_eq!(page["total_pages"], 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn upload_rate_limit_20_per_jam(pool: PgPool) {
    let router = build_app(pool).await;
    let (token, ip) = register_user(&router, "limited@example.com").await;

    // File invalid cepat ditolak (tanpa ffprobe) tapi tetap dihitung limiter.
    let teks = vec![b'x'; 128];
    let mut got_429 = false;
    for i in 0..22 {
        let (status, _) = upload(&router, &token, &ip, "bukan.txt", &teks, None).await;
        if status == StatusCode::TOO_MANY_REQUESTS {
            got_429 = true;
            assert!(i >= 20, "429 hanya setelah 20 upload (ke-{i})");
            break;
        }
        assert_eq!(status, StatusCode::BAD_REQUEST, "upload ke-{i}");
    }
    assert!(got_429, "harus kena 429 dalam 22 upload");
}
