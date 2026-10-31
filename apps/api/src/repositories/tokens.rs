//! Query tabel `refresh_tokens` — rotasi & reuse-detection (docs/08 §1).

use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
pub struct RefreshTokenRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub expires_at: OffsetDateTime,
    pub revoked_at: Option<OffsetDateTime>,
}

#[allow(clippy::too_many_arguments)]
pub async fn insert(
    db: &PgPool,
    id: Uuid,
    user_id: Uuid,
    token_hash: &str,
    expires_at: OffsetDateTime,
    user_agent: Option<&str>,
    ip: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        // $6::inet — param teks perlu cast eksplisit (kolom inet)
        "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, user_agent, ip)
         VALUES ($1, $2, $3, $4, $5, $6::inet)",
    )
    .bind(id)
    .bind(user_id)
    .bind(token_hash)
    .bind(expires_at)
    .bind(user_agent)
    .bind(ip)
    .execute(db)
    .await?;
    Ok(())
}

/// Cari by hash TANPA filter — supaya token yang sudah revoked tetap terdeteksi (reuse).
pub async fn find_by_hash(
    db: &PgPool,
    token_hash: &str,
) -> Result<Option<RefreshTokenRow>, sqlx::Error> {
    sqlx::query_as::<_, RefreshTokenRow>(
        "SELECT id, user_id, token_hash, expires_at, revoked_at
         FROM refresh_tokens WHERE token_hash = $1",
    )
    .bind(token_hash)
    .fetch_optional(db)
    .await
}

pub async fn revoke(db: &PgPool, id: Uuid, now: OffsetDateTime) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE refresh_tokens SET revoked_at = $2 WHERE id = $1 AND revoked_at IS NULL")
        .bind(id)
        .bind(now)
        .execute(db)
        .await?;
    Ok(())
}

/// Reuse terdeteksi → cabut SEMUA sesi user (docs/08 §1).
pub async fn revoke_all_for_user(db: &PgPool, user_id: Uuid) -> Result<u64, sqlx::Error> {
    let res = sqlx::query(
        "UPDATE refresh_tokens SET revoked_at = now()
         WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(res.rows_affected())
}
