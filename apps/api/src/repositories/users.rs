//! Query tabel `users` — docs/04-database-design.md.

use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::user::UserRow;

const COLUMNS: &str = "id, email, password_hash, name, avatar_url, role::text AS role, plan::text AS plan, created_at";

pub async fn insert(
    db: &PgPool,
    id: Uuid,
    email: &str,
    password_hash: &str,
    name: &str,
) -> Result<UserRow, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(&format!(
        "INSERT INTO users (id, email, password_hash, name)
         VALUES ($1, $2, $3, $4)
         RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(email)
    .bind(password_hash)
    .bind(name)
    .fetch_one(db)
    .await
}

pub async fn find_by_email(db: &PgPool, email: &str) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {COLUMNS} FROM users
         WHERE email = $1 AND deleted_at IS NULL"
    ))
    .bind(email)
    .fetch_optional(db)
    .await
}

pub async fn find_by_id(db: &PgPool, id: Uuid) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {COLUMNS} FROM users
         WHERE id = $1 AND deleted_at IS NULL"
    ))
    .bind(id)
    .fetch_optional(db)
    .await
}
