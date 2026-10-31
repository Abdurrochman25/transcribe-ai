//! Shared state aplikasi — lihat docs/05-backend-design.md §2.

use std::sync::Arc;

use sqlx::PgPool;

use crate::config::Config;
use crate::infra::groq::GroqClient;
use crate::infra::sse::SseHub;
use crate::infra::storage::Storage;
use crate::middleware::rate_limit::RateLimiter;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub cfg: Arc<Config>,
    pub storage: Storage,
    /// Rate limit upload: 20/jam/user (docs/08 §1).
    pub upload_limiter: RateLimiter,
    pub groq: GroqClient,
    pub sse: SseHub,
}

impl AppState {
    /// Konstruktor untuk test: storage local sementara + limiter upload standar.
    /// GroqClient mengikuti `cfg.groq_base_url` (arahkan ke wiremock di test).
    pub fn for_test(db: PgPool, cfg: Config) -> Self {
        let groq = GroqClient::for_test(
            cfg.groq_api_key.as_deref().unwrap_or("test-key"),
            &cfg.groq_base_url,
        )
        .expect("groq client");
        Self {
            db,
            cfg: Arc::new(cfg),
            storage: Storage::local_for_test(
                &std::env::temp_dir().join(format!("transcribe-test-{}", uuid::Uuid::now_v7())),
            ),
            upload_limiter: RateLimiter::new(20, std::time::Duration::from_secs(3600)),
            groq,
            sse: SseHub::new(),
        }
    }
}
