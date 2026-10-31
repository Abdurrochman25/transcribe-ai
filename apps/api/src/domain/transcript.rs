//! Entity transcript — docs/04-database-design.md.

use serde::Serialize;
use time::OffsetDateTime;
use uuid::Uuid;

pub const MODEL_TURBO: &str = "whisper-large-v3-turbo";
pub const MODEL_LARGE: &str = "whisper-large-v3";

pub fn allowed_models() -> &'static [&'static str] {
    &[MODEL_TURBO, MODEL_LARGE]
}

#[derive(Debug, sqlx::FromRow)]
pub struct TranscriptRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub title: String,
    pub source_filename: String,
    pub size_bytes: i64,
    pub mime_type: String,
    pub storage_key: String,
    pub duration_seconds: Option<f64>,
    pub language: Option<String>,
    pub requested_language: Option<String>,
    pub model: String,
    pub prompt_hint: Option<String>,
    pub status: String,
    pub progress: i16,
    pub created_at: OffsetDateTime,
    pub completed_at: Option<OffsetDateTime>,
}

/// Representasi publik (list/detail) — docs/03.
#[derive(Debug, Serialize)]
pub struct TranscriptPublic {
    pub id: Uuid,
    pub title: String,
    pub status: String,
    pub progress: i16,
    pub source_filename: String,
    pub size_bytes: i64,
    pub mime_type: String,
    pub duration_seconds: Option<f64>,
    pub language: Option<String>,
    pub requested_language: Option<String>,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_hint: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(
        with = "time::serde::rfc3339::option",
        skip_serializing_if = "Option::is_none"
    )]
    pub completed_at: Option<OffsetDateTime>,
}

impl TranscriptRow {
    pub fn to_public(&self) -> TranscriptPublic {
        TranscriptPublic {
            id: self.id,
            title: self.title.clone(),
            status: self.status.clone(),
            progress: self.progress,
            source_filename: self.source_filename.clone(),
            size_bytes: self.size_bytes,
            mime_type: self.mime_type.clone(),
            duration_seconds: self.duration_seconds,
            language: self.language.clone(),
            requested_language: self.requested_language.clone(),
            model: self.model.clone(),
            prompt_hint: self.prompt_hint.clone(),
            created_at: self.created_at,
            completed_at: self.completed_at,
        }
    }
}
