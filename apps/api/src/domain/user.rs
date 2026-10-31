//! Entity user — representasi DB (`UserRow`) dan publik (`UserPublic`).

use serde::Serialize;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub name: String,
    pub avatar_url: Option<String>,
    pub role: String,
    pub plan: String,
    pub created_at: OffsetDateTime,
}

impl UserRow {
    pub fn to_public(&self) -> UserPublic {
        UserPublic {
            id: self.id,
            email: self.email.clone(),
            name: self.name.clone(),
            plan: self.plan.clone(),
            created_at: self.created_at,
        }
    }
}

/// Representasi user yang aman dikirim ke klien (tanpa hash password).
#[derive(Debug, Serialize)]
pub struct UserPublic {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub plan: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}
