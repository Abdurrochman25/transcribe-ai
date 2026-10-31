//! Integration test auth — in-proc router (oneshot) + DB ephemeral per-test.
//! Menjalankan: butuh Postgres lokal (`podman compose up -d postgres`), DATABASE_URL
//! dibaca dari apps/api/.env oleh dotenvy.

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
    // Tampilkan error internal (query dsb.) saat test — with_test_writer.
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
        max_upload_bytes: 1024 * 1024,
        ffprobe_path: "ffprobe".into(),
    }
}

async fn build_app(pool: PgPool) -> axum::Router {
    app(AppState::for_test(pool, test_config()))
}

/// Key unik per-test supaya rate limiter (per instance router) tidak saling ganggu.
/// Rate limiter hanya memakai ini sebagai key — TIDAK diteruskan ke DB (ip harus valid).
fn ip_key() -> String {
    format!(
        "10.{}.{}.{}",
        rand_u32() % 250,
        rand_u32() % 250,
        rand_u32() % 250 + 1
    )
}

fn rand_u32() -> u32 {
    uuid::Uuid::now_v7().to_u128_le() as u32
}

async fn post_json(
    router: &axum::Router,
    uri: &str,
    body: Value,
    ip: &str,
) -> (StatusCode, Option<Value>, Option<String>) {
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-forwarded-for", ip)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    let set_cookie = res
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|c| c.split(';').next())
        .map(str::to_string);
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = if bytes.is_empty() {
        None
    } else {
        Some(serde_json::from_slice(&bytes).unwrap())
    };
    (status, json, set_cookie)
}

async fn get_json(
    router: &axum::Router,
    uri: &str,
    bearer: Option<&str>,
) -> (StatusCode, Option<Value>) {
    let mut builder = Request::builder().uri(uri);
    if let Some(token) = bearer {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let res = router
        .clone()
        .oneshot(builder.body(Body::empty()).unwrap())
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

fn cookie_header(set_cookie: &str) -> String {
    set_cookie.split(';').next().unwrap().to_string()
}

fn register_body(email: &str) -> Value {
    json!({ "email": email, "password": "rahasia-aman-2026", "name": "Andi Tester" })
}

// ===== Tests =====

#[sqlx::test(migrations = "./migrations")]
async fn full_flow_register_login_me_refresh_logout(pool: PgPool) {
    let router = build_app(pool).await;
    let ip = ip_key();

    // 1. Register → 201 + user + tokens + cookie rt.
    let (status, body, cookie) = post_json(
        &router,
        "/api/v1/auth/register",
        register_body("andi@example.com"),
        &ip,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "register: {body:?}");
    let access = body.as_ref().unwrap()["tokens"]["access_token"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(body.as_ref().unwrap()["user"]["email"], "andi@example.com");
    assert!(cookie.as_deref().is_some_and(|c| c.starts_with("rt=")));

    // 2. Login ulang → 200 (user sama).
    let (status, body, _) = post_json(
        &router,
        "/api/v1/auth/login",
        json!({ "email": "andi@example.com", "password": "rahasia-aman-2026" }),
        &ip,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "login: {body:?}");

    // 3. /auth/me dengan Bearer → profil.
    let (status, body) = get_json(&router, "/api/v1/auth/me", Some(&access)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_ref().unwrap()["user"]["name"], "Andi Tester");

    // 4. /auth/me tanpa token → 401 envelope.
    let (status, body) = get_json(&router, "/api/v1/auth/me", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body.as_ref().unwrap()["error"]["code"], "UNAUTHENTICATED");

    // 5. Refresh dengan cookie → 200 tokens baru (rotasi).
    let (status, body, new_cookie) = post_json_with_cookie(
        &router,
        "/api/v1/auth/refresh",
        json!({}),
        &ip,
        cookie.as_deref().unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "refresh: {body:?}");
    assert!(
        body.as_ref().unwrap()["tokens"]["access_token"]
            .as_str()
            .is_some()
    );
    assert!(new_cookie.is_some(), "rotasi menghasilkan cookie baru");
}

#[sqlx::test(migrations = "./migrations")]
async fn register_validasi_duplikat_dan_login_salah(pool: PgPool) {
    let router = build_app(pool).await;
    let ip = ip_key();

    // Password lemah → 400 VALIDATION_ERROR dengan details.
    let (status, body, _) = post_json(
        &router,
        "/api/v1/auth/register",
        json!({ "email": "b@example.com", "password": "pendek1", "name": "B" }),
        &ip,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err = &body.as_ref().unwrap()["error"];
    assert_eq!(err["code"], "VALIDATION_ERROR");
    assert!(err["details"].as_array().is_some_and(|d| !d.is_empty()));

    // Email buruk → 400.
    let (status, _, _) = post_json(
        &router,
        "/api/v1/auth/register",
        json!({ "email": "bukan-email", "password": "rahasia-aman-2026", "name": "B" }),
        &ip,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Sukses pertama → 201; duplikat → 409.
    let (status, _, _) = post_json(
        &router,
        "/api/v1/auth/register",
        register_body("bunga@example.com"),
        &ip,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, body, _) = post_json(
        &router,
        "/api/v1/auth/register",
        register_body("bunga@example.com"),
        &ip,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");

    // Login password salah → 401 pesan generik.
    let (status, body, _) = post_json(
        &router,
        "/api/v1/auth/login",
        json!({ "email": "bunga@example.com", "password": "salah-total-999" }),
        &ip,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let msg = body.as_ref().unwrap()["error"]["message"].as_str().unwrap();
    assert!(
        msg.contains("Email atau password salah"),
        "pesan generik: {msg}"
    );

    // Login email tidak terdaftar → 401 dengan pesan SAMA (anti enumeration).
    let (status, body2, _) = post_json(
        &router,
        "/api/v1/auth/login",
        json!({ "email": "hantu@example.com", "password": "salah-total-999" }),
        &ip,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        body.as_ref().unwrap()["error"]["message"],
        body2.as_ref().unwrap()["error"]["message"],
        "pesan login gagal harus identik"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn refresh_rotasi_dan_reuse_detection(pool: PgPool) {
    let router = build_app(pool).await;
    let ip = ip_key();

    let (_, _, cookie) = post_json(
        &router,
        "/api/v1/auth/register",
        register_body("rafi@example.com"),
        &ip,
    )
    .await;
    let cookie = cookie.unwrap();

    // Rotasi: refresh pertama sukses.
    let (status, _, cookie2) =
        post_json_with_cookie(&router, "/api/v1/auth/refresh", json!({}), &ip, &cookie).await;
    assert_eq!(status, StatusCode::OK);

    // REUSE: pakai cookie LAMA lagi → 401.
    let (status, body, _) =
        post_json_with_cookie(&router, "/api/v1/auth/refresh", json!({}), &ip, &cookie).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body:?}");

    // Semua sesi user ikut dicabut → cookie BARU pun 401.
    let (status, _, _) = post_json_with_cookie(
        &router,
        "/api/v1/auth/refresh",
        json!({}),
        &ip,
        &cookie2.unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "semua sesi harus tercabut setelah reuse"
    );

    // Login kembali → dapat sesi baru (user tidak terkunci permanen).
    let (status, _, _) = post_json(
        &router,
        "/api/v1/auth/login",
        json!({ "email": "rafi@example.com", "password": "rahasia-aman-2026" }),
        &ip,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test(migrations = "./migrations")]
async fn logout_mencabut_refresh(pool: PgPool) {
    let router = build_app(pool).await;
    let ip = ip_key();

    let (status, body, cookie) = post_json(
        &router,
        "/api/v1/auth/register",
        register_body("out@example.com"),
        &ip,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let access = body.unwrap()["tokens"]["access_token"]
        .as_str()
        .unwrap()
        .to_string();
    let cookie = cookie.unwrap();

    // Logout dengan Bearer + cookie → 204.
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/logout")
                .header(header::AUTHORIZATION, format!("Bearer {access}"))
                .header(header::COOKIE, cookie_header(&cookie))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // Refresh dengan cookie yang sudah dicabut → 401.
    let (status, _, _) =
        post_json_with_cookie(&router, "/api/v1/auth/refresh", json!({}), &ip, &cookie).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn rate_limit_auth_per_ip(pool: PgPool) {
    let router = build_app(pool).await;
    let ip = ip_key();

    // 10 permintaan pertama lolos (401 salah password), ke-11 → 429.
    let mut got_429 = false;
    for i in 0..12 {
        let (status, _, _) = post_json(
            &router,
            "/api/v1/auth/login",
            json!({ "email": "x@example.com", "password": "salah-total-999" }),
            &ip,
        )
        .await;
        if status == StatusCode::TOO_MANY_REQUESTS {
            got_429 = true;
            assert!(i >= 10, "429 hanya boleh setelah 10 permintaan (ke-{i})");
            break;
        }
        assert_eq!(status, StatusCode::UNAUTHORIZED, "permintaan ke-{i}");
    }
    assert!(got_429, "harus kena 429 dalam 12 permintaan");
}

// ===== Helper tambahan =====

async fn post_json_with_cookie(
    router: &axum::Router,
    uri: &str,
    body: Value,
    ip: &str,
    cookie: &str,
) -> (StatusCode, Option<Value>, Option<String>) {
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie_header(cookie))
                .header("x-forwarded-for", ip)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let set_cookie = res
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|c| c.split(';').next())
        .map(str::to_string);
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = if bytes.is_empty() {
        None
    } else {
        Some(serde_json::from_slice(&bytes).unwrap())
    };
    (status, json, set_cookie)
}
