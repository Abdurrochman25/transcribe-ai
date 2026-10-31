//! Konfigurasi aplikasi — seluruhnya dari environment, fail-fast saat start.
//! Lihat docs/05-backend-design.md §3.

use std::env;

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Text,
    Json,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub jwt_secret: String,
    /// TODO(fase-3): menjadi wajib saat worker transkripsi aktif.
    pub groq_api_key: Option<String>,
    pub groq_base_url: String,
    pub cors_allowed_origins: Vec<String>,
    pub log_format: LogFormat,
    pub access_token_ttl_secs: i64,
    pub refresh_token_ttl_secs: i64,
    /// Set true di produksi (HTTPS) agar cookie refresh hanya dikirim via https.
    pub cookie_secure: bool,
}

impl Config {
    /// Membaca konfigurasi dari environment. Gagal (error jelas) bila nilai
    /// wajib kosong atau tidak valid — sesuai prinsip fail-fast.
    pub fn from_env() -> Result<Self> {
        let database_url = env_required("DATABASE_URL")?;
        let jwt_secret = env_required("JWT_SECRET")?;
        if jwt_secret.len() < 32 {
            bail!(
                "JWT_SECRET harus minimal 32 byte acak (ditemukan {} byte) — lihat docs/08-security.md",
                jwt_secret.len()
            );
        }

        // TODO(fase-3): GROQ_API_KEY menjadi wajib saat worker transkripsi aktif.
        let groq_api_key = env::var("GROQ_API_KEY")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let config = Self {
            port: env_parse("PORT", 8080)?,
            database_url,
            jwt_secret,
            groq_api_key,
            groq_base_url: env_string_or("GROQ_BASE_URL", "https://api.groq.com/openai/v1"),
            cors_allowed_origins: env_list_or(
                "CORS_ALLOWED_ORIGINS",
                vec!["http://localhost:5173".to_string()],
            ),
            log_format: match env::var("LOG_FORMAT").as_deref() {
                Ok("json") => LogFormat::Json,
                _ => LogFormat::Text,
            },
            access_token_ttl_secs: env_parse("ACCESS_TOKEN_TTL_SECS", 900)?,
            refresh_token_ttl_secs: env_parse("REFRESH_TOKEN_TTL_SECS", 2_592_000)?,
            cookie_secure: env_parse("COOKIE_SECURE", false)?,
        };
        Ok(config)
    }
}

fn env_required(name: &str) -> Result<String> {
    let value = env::var(name)
        .with_context(|| format!("environment variable `{name}` wajib di-set"))?
        .trim()
        .to_string();
    if value.is_empty() {
        bail!("environment variable `{name}` tidak boleh kosong");
    }
    Ok(value)
}

fn env_string_or(name: &str, default: &str) -> String {
    env::var(name)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn env_list_or(name: &str, default: Vec<String>) -> Vec<String> {
    let raw = env::var(name).unwrap_or_default();
    let items: Vec<String> = raw
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if items.is_empty() { default } else { items }
}

fn env_parse<T: std::str::FromStr>(name: &str, default: T) -> Result<T> {
    match env::var(name) {
        Ok(s) if !s.trim().is_empty() => s
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("environment variable `{name}` tidak valid")),
        _ => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catatan: mutasi env harus serial — kumpulkan assertion dalam satu test.
    // Edition 2024: set_var unsafe.
    #[test]
    fn config_validasi_wajib() {
        unsafe {
            env::set_var(
                "DATABASE_URL",
                "postgres://app:dev@localhost:5432/transcribe",
            );
            env::set_var("JWT_SECRET", "pendek");
        }
        let err = Config::from_env().unwrap_err();
        assert!(err.to_string().contains("JWT_SECRET"));

        unsafe {
            env::set_var("JWT_SECRET", "0123456789abcdef0123456789abcdef");
            env::set_var("PORT", "8090");
        }
        let cfg = Config::from_env().unwrap();
        assert_eq!(cfg.port, 8090);
        assert_eq!(cfg.cors_allowed_origins, vec!["http://localhost:5173"]);

        unsafe {
            env::set_var("PORT", "bukan-angka");
        }
        assert!(Config::from_env().is_err());
    }
}
