//! Rate limiting sederhana per-key sliding window (docs/03 §Rate Limiting).
//! Key = IP (ConnectInfo / x-forwarded-for). Fase ini dipakai untuk endpoint auth
//! (10 permintaan/menit/IP). Upgrade path: `governor` bila butuh limit global/distributed.

use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::Request,
    http::{HeaderValue, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;

#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<RateLimiterInner>,
}

struct RateLimiterInner {
    hits: DashMap<String, VecDeque<Instant>>,
    limit: usize,
    window: Duration,
}

impl RateLimiter {
    pub fn new(limit: usize, window: Duration) -> Self {
        Self {
            inner: Arc::new(RateLimiterInner {
                hits: DashMap::new(),
                limit,
                window,
            }),
        }
    }

    /// Ok(()) bila boleh lewat; Err(retry_after_secs) bila kena limit.
    pub fn check(&self, key: &str) -> Result<(), u64> {
        let now = Instant::now();
        let cutoff = now.checked_sub(self.inner.window);

        let mut entry = self.inner.hits.entry(key.to_string()).or_default();
        let hits = entry.value_mut();

        if let Some(cutoff) = cutoff {
            while hits.front().is_some_and(|t| *t < cutoff) {
                hits.pop_front();
            }
        }

        if hits.len() >= self.inner.limit {
            // Hitung detik sampai slot tertua keluar dari jendela.
            let retry_after = hits
                .front()
                .map(|t| self.inner.window.saturating_sub(t.elapsed()).as_secs() + 1)
                .unwrap_or(1);
            return Err(retry_after);
        }

        hits.push_back(now);
        Ok(())
    }
}

/// Middleware guard — pasang via `axum::middleware::from_fn` dengan closure yang
/// menangkap `RateLimiter` (lihat routes/auth.rs).
pub async fn guard(limiter: RateLimiter, req: Request, next: Next) -> Response {
    let key = client_key(&req);
    if let Err(retry_after) = limiter.check(&key) {
        let mut res = crate::error::AppError::RateLimited.into_response();
        if let Ok(v) = HeaderValue::from_str(&retry_after.to_string()) {
            res.headers_mut().insert(header::RETRY_AFTER, v);
        }
        tracing::warn!(key = %key, retry_after, "rate limit aktif");
        return res;
    }
    next.run(req).await
}

fn client_key(req: &Request) -> String {
    // 1) IP peer asli (dari into_make_service_with_connect_info)
    if let Some(info) = req
        .extensions()
        .get::<axum::extract::connect_info::ConnectInfo<std::net::SocketAddr>>()
    {
        return info.0.ip().to_string();
    }
    // 2) Proxy header (dev/CI)
    if let Some(fwd) = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        return fwd.to_string();
    }
    "unknown".to_string()
}
