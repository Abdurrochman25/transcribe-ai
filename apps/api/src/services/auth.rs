//! Logika auth: register, login, refresh (rotasi + reuse-detection), logout.
//! Spesifikasi kontrak: docs/03-api-spec.md §Auth. Kebijakan: docs/08-security.md §1.

use argon2::{
    Argon2, PasswordHasher, PasswordVerifier,
    password_hash::{PasswordHash, SaltString, rand_core::OsRng},
};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::config::Config;
use crate::domain::user::UserPublic;
use crate::error::{AppError, FieldError};
use crate::repositories;

pub const MSG_PERLU_LOGIN: &str = "Autentikasi diperlukan";
pub const MSG_SESI_TIDAK_VALID: &str = "Sesi tidak valid, silakan login kembali";
pub const MSG_LOGIN_GAGAL: &str = "Email atau password salah";

/// Password yang terlalu umum — ditolak (docs/01: blok `password123`-class).
const PASSWORD_BLACKLIST: &[&str] = &[
    "password1234",
    "password123456",
    "1234567890",
    "qwertyuiop",
    "qwerty12345",
    "indonesia123",
    "rahasia123",
    "admin123456",
];

#[derive(Debug, Deserialize)]
pub struct RegisterInput {
    pub email: String,
    pub password: String,
    pub name: String,
}

pub struct AuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
}

pub struct AuthOutcome {
    pub user: UserPublic,
    pub tokens: AuthTokens,
}

/// Claims JWT access token — payload minimalis (docs/08 §1).
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub jti: String,
    pub iat: u64,
    pub exp: u64,
}

// ===== Endpoint logic =====

pub async fn register(
    db: &PgPool,
    cfg: &Config,
    input: RegisterInput,
    user_agent: Option<&str>,
    ip: Option<&str>,
) -> Result<AuthOutcome, AppError> {
    validate_register(&input)?;

    // Cek dulu agar pesan 409 jelas; race condition tetap ditangani unique constraint di bawah.
    if repositories::users::find_by_email(db, input.email.trim())
        .await?
        .is_some()
    {
        return Err(AppError::Conflict("email sudah terdaftar"));
    }

    let password_hash = hash_password(&input.password)?;
    let user = match repositories::users::insert(
        db,
        Uuid::now_v7(),
        input.email.trim(),
        &password_hash,
        input.name.trim(),
    )
    .await
    {
        Ok(user) => user,
        Err(err) if is_unique_violation(&err) => {
            return Err(AppError::Conflict("email sudah terdaftar"));
        }
        Err(err) => return Err(err.into()),
    };

    issue_session(db, cfg, &user, user_agent, ip).await
}

pub async fn login(
    db: &PgPool,
    cfg: &Config,
    email: &str,
    password: &str,
    user_agent: Option<&str>,
    ip: Option<&str>,
) -> Result<AuthOutcome, AppError> {
    // Pesan gagal selalu generik — anti user-enumeration (docs/08 §1).
    let user = repositories::users::find_by_email(db, email.trim())
        .await?
        .ok_or(AppError::Unauthenticated(MSG_LOGIN_GAGAL))?;

    let parsed = PasswordHash::new(&user.password_hash)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("password hash korup")))?;
    if Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_err()
    {
        return Err(AppError::Unauthenticated(MSG_LOGIN_GAGAL));
    }

    issue_session(db, cfg, &user, user_agent, ip).await
}

/// Refresh dengan ROTASI: token lama dicabut, token baru diterbitkan.
/// REUSE-DETECTION: token yang sudah dicabut dipakai lagi → semua sesi user dicabut.
pub async fn refresh(
    db: &PgPool,
    cfg: &Config,
    raw_refresh_token: &str,
    user_agent: Option<&str>,
    ip: Option<&str>,
) -> Result<AuthOutcome, AppError> {
    let hash = sha256_hex(raw_refresh_token);
    let row = repositories::tokens::find_by_hash(db, &hash)
        .await?
        .ok_or(AppError::Unauthenticated(MSG_SESI_TIDAK_VALID))?;

    if row.revoked_at.is_some() {
        let n = repositories::tokens::revoke_all_for_user(db, row.user_id).await?;
        tracing::warn!(
            user_id = %row.user_id,
            revoked = n,
            "reuse refresh token terdeteksi — semua sesi user dicabut"
        );
        return Err(AppError::Unauthenticated(MSG_SESI_TIDAK_VALID));
    }

    if row.expires_at <= OffsetDateTime::now_utc() {
        return Err(AppError::Unauthenticated(MSG_SESI_TIDAK_VALID));
    }

    let user = repositories::users::find_by_id(db, row.user_id)
        .await?
        .ok_or(AppError::Unauthenticated(MSG_SESI_TIDAK_VALID))?;

    // Rotasi.
    repositories::tokens::revoke(db, row.id, OffsetDateTime::now_utc()).await?;

    issue_session(db, cfg, &user, user_agent, ip).await
}

pub async fn logout(
    db: &PgPool,
    user_id: Uuid,
    raw_refresh_token: Option<&str>,
) -> Result<(), AppError> {
    if let Some(raw) = raw_refresh_token {
        let hash = sha256_hex(raw);
        if let Some(row) = repositories::tokens::find_by_hash(db, &hash).await?
            && row.user_id == user_id
        {
            repositories::tokens::revoke(db, row.id, OffsetDateTime::now_utc()).await?;
        }
    }
    Ok(())
}

// ===== Helper =====

async fn issue_session(
    db: &PgPool,
    cfg: &Config,
    user: &crate::domain::user::UserRow,
    user_agent: Option<&str>,
    ip: Option<&str>,
) -> Result<AuthOutcome, AppError> {
    let access_token = issue_access(cfg, user.id);
    let (raw, hash) = generate_refresh();
    repositories::tokens::insert(
        db,
        Uuid::now_v7(),
        user.id,
        &hash,
        OffsetDateTime::now_utc() + Duration::seconds(cfg.refresh_token_ttl_secs),
        user_agent,
        ip,
    )
    .await?;

    Ok(AuthOutcome {
        user: user.to_public(),
        tokens: AuthTokens {
            access_token,
            refresh_token: raw,
            expires_in: cfg.access_token_ttl_secs,
        },
    })
}

pub fn issue_access(cfg: &Config, user_id: Uuid) -> String {
    let now = OffsetDateTime::now_utc().unix_timestamp() as u64;
    let claims = Claims {
        sub: user_id.to_string(),
        jti: Uuid::now_v7().to_string(),
        iat: now,
        exp: now + cfg.access_token_ttl_secs.max(1) as u64,
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(cfg.jwt_secret.as_bytes()),
    )
    .expect("JWT encode tidak mungkin gagal untuk claim valid")
}

/// Refresh token acak 256-bit; DB hanya menyimpan SHA-256 hash-nya (docs/08 §1).
pub fn generate_refresh() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let raw = URL_SAFE_NO_PAD.encode(bytes);
    let hash = sha256_hex(&raw);
    (raw, hash)
}

pub fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex_lower(&hasher.finalize())
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hash_password(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("argon2 hash gagal: {e}")))
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    err.as_database_error()
        .is_some_and(|db| db.is_unique_violation())
}

fn validate_register(input: &RegisterInput) -> Result<(), AppError> {
    let mut errors = Vec::new();

    let email = input.email.trim();
    if !email_valid(email) {
        errors.push(FieldError {
            field: "email".into(),
            issue: "format email tidak valid".into(),
        });
    }

    let password_errors = validate_password(&input.password);
    errors.extend(password_errors);

    let name = input.name.trim();
    if name.is_empty() || name.len() > 100 {
        errors.push(FieldError {
            field: "name".into(),
            issue: "nama wajib 1-100 karakter".into(),
        });
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::Validation(errors))
    }
}

fn email_valid(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !email.contains(char::is_whitespace)
}

fn validate_password(password: &str) -> Vec<FieldError> {
    let mut errors = Vec::new();
    if password.len() < 10 {
        errors.push(FieldError {
            field: "password".into(),
            issue: "minimal 10 karakter".into(),
        });
    }
    if password.len() > 128 {
        errors.push(FieldError {
            field: "password".into(),
            issue: "maksimal 128 karakter".into(),
        });
    }
    if !password.chars().any(char::is_alphabetic) || !password.chars().any(|c| c.is_ascii_digit()) {
        errors.push(FieldError {
            field: "password".into(),
            issue: "harus mengandung huruf dan angka".into(),
        });
    }
    if PASSWORD_BLACKLIST.contains(&password.to_lowercase().as_str()) {
        errors.push(FieldError {
            field: "password".into(),
            issue: "password ini terlalu umum, pilih yang lain".into(),
        });
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validasi_email() {
        assert!(email_valid("andi@example.com"));
        assert!(email_valid("a.b+c@sub.domain.id"));
        assert!(!email_valid("tidakada"));
        assert!(!email_valid("@domain.com"));
        assert!(!email_valid("user@domain"));
        assert!(!email_valid("user @domain.com"));
    }

    #[test]
    fn validasi_password_rules() {
        assert!(validate_password("rahasia-aman-2026").is_empty());
        assert!(!validate_password("pendek1").is_empty(), "terlalu pendek");
        assert!(
            !validate_password("tanpaangkasekali").is_empty(),
            "wajib ada angka"
        );
        assert!(
            !validate_password("1234567890").is_empty(),
            "wajib ada huruf"
        );
        assert!(
            !validate_password("password1234").is_empty(),
            "blacklist harus ditolak"
        );
    }

    #[test]
    fn refresh_token_format_dan_hash() {
        let (raw, hash) = generate_refresh();
        assert_eq!(raw.len(), 43, "32 byte → base64url tanpa pad = 43 char");
        assert_ne!(raw, hash);
        assert_eq!(hash.len(), 64, "sha256 hex = 64 char");
        assert_eq!(sha256_hex(&raw), hash, "hash deterministik");
    }

    #[test]
    fn access_token_bisa_dekode() {
        let cfg = test_cfg();
        let token = issue_access(&cfg, Uuid::now_v7());
        let data = jsonwebtoken::decode::<Claims>(
            &token,
            &jsonwebtoken::DecodingKey::from_secret(cfg.jwt_secret.as_bytes()),
            &jsonwebtoken::Validation::default(),
        )
        .expect("decode");
        assert_eq!(data.claims.exp - data.claims.iat, 900);
    }

    fn test_cfg() -> Config {
        Config {
            port: 0,
            database_url: String::new(),
            jwt_secret: "test-secret-32-byte-untuk-unit-test!".into(),
            groq_api_key: None,
            groq_base_url: String::new(),
            cors_allowed_origins: vec![],
            log_format: crate::config::LogFormat::Text,
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
}
