//! Security headers HTTP untuk semua respons — docs/08 §3.
//! HSTS hanya saat TLS aktif (COOKIE_SECURE=true, TLS dihentikan proxy di depan)
//! agar lingkungan dev http:// tidak dirusak.

use axum::{
    extract::{Request, State},
    http::{HeaderValue, header},
    middleware::Next,
    response::Response,
};

use crate::state::AppState;

pub async fn security_headers(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let headers = res.headers_mut();

    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    if state.cfg.cookie_secure {
        // 180 hari + includeSubDomains — dok 08 §3.
        headers.insert(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=15552000; includeSubDomains"),
        );
    }
    res
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{Router, body::Body, http::Request, routing::get};
    use tower::ServiceExt;

    use crate::state::AppState;

    async fn ping() -> &'static str {
        "pong"
    }

    #[tokio::test]
    async fn header_aman_aktif_hsts_ikut_cookie_secure() {
        for (secure, expect_hsts) in [(false, false), (true, true)] {
            let mut cfg = crate::config::Config::default_for_test();
            cfg.cookie_secure = secure;
            // DB lazy — middleware tidak menyentuh db.
            let db = sqlx::PgPool::connect_lazy("postgres://t:t@localhost:5432/t").unwrap();
            let state = AppState {
                db,
                cfg: Arc::new(cfg),
                storage: crate::infra::storage::Storage::local_for_test(&std::env::temp_dir()),
                upload_limiter: crate::middleware::rate_limit::RateLimiter::new(
                    20,
                    std::time::Duration::from_secs(3600),
                ),
                groq: crate::infra::groq::GroqClient::for_test("t", "http://127.0.0.1:9").unwrap(),
                sse: crate::infra::sse::SseHub::new(),
            };
            let app = Router::new()
                .route("/ping", get(ping))
                .layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    super::security_headers,
                ))
                .with_state(state);

            let res = app
                .oneshot(Request::builder().uri("/ping").body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(
                res.headers()["x-content-type-options"],
                "nosniff",
                "nosniff selalu ada"
            );
            assert_eq!(res.headers()["x-frame-options"], "DENY");
            assert_eq!(
                res.headers()["referrer-policy"],
                "strict-origin-when-cross-origin"
            );
            assert_eq!(
                res.headers().contains_key("strict-transport-security"),
                expect_hsts,
                "HSTS hanya saat secure={secure}"
            );
        }
    }
}
