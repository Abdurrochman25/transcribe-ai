//! Shared state aplikasi — lihat docs/05-backend-design.md §2.

use std::sync::Arc;

use sqlx::PgPool;

use crate::config::Config;
use crate::infra::storage::Storage;
use crate::middleware::rate_limit::RateLimiter;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub cfg: Arc<Config>,
    pub storage: Storage,
    /// Rate limit upload: 20/jam/user (docs/08 §1).
    pub upload_limiter: RateLimiter,
}

impl AppState {
    /// Konstruktor untuk test: storage local sementara + limiter upload standar.
    pub fn for_test(db: PgPool, cfg: Config) -> Self {
        let root = std::env::temp_dir().join(format!("transcribe-test-{}", uuid::Uuid::now_v7()));
        Self {
            db,
            cfg: Arc::new(cfg),
            storage: Storage::local_for_test(&root),
            upload_limiter: RateLimiter::new(20, std::time::Duration::from_secs(3600)),
        }
    }
}
