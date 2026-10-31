//! Klien Groq Whisper API (OpenAI-compatible) — docs/05 §5.
//! Retry: 429/5xx → exponential backoff + jitter, hormati Retry-After.

use anyhow::Result;
use rand::Rng;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct GroqClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    max_retries: u32,
    retry_base_ms: u64,
}

#[derive(Debug)]
pub struct TranscribeParams<'a> {
    pub model: &'a str,
    pub language: Option<&'a str>,
    pub prompt: Option<&'a str>,
    pub filename: &'a str,
    pub mime: &'a str,
}

/// Segment dari `verbose_json` Groq.
#[derive(Debug, Clone, Deserialize)]
pub struct GroqSegment {
    #[serde(default)]
    pub id: Option<i64>,
    pub start: f64,
    pub end: f64,
    pub text: String,
    #[serde(default)]
    pub no_speech_prob: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroqVerbose {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub language_probability: Option<f64>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub segments: Vec<GroqSegment>,
}

#[derive(Debug, thiserror::Error)]
pub enum GroqError {
    #[error("retryable [{status}]: {message}")]
    Retryable {
        status: u16,
        message: String,
        retry_after: Option<u64>,
    },
    #[error("non-retryable [{status}]: {message}")]
    NonRetryable { status: u16, message: String },
    #[error("network: {0}")]
    Network(String),
}

impl GroqError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, GroqError::Retryable { .. } | GroqError::Network(_))
    }

    /// Pesan ramah untuk disimpan di kolom `transcripts.error`.
    pub fn friendly(&self) -> String {
        match self {
            GroqError::Retryable {
                status,
                retry_after: Some(s),
                ..
            } => {
                format!("Layanan transkripsi sibuk (HTTP {status}), coba lagi setelah ~{s}s")
            }
            GroqError::Retryable { status, .. } => {
                format!("Layanan transkripsi bermasalah (HTTP {status})")
            }
            GroqError::NonRetryable { message, .. } => {
                format!("Ditolak layanan transkripsi: {message}")
            }
            GroqError::Network(_) => "Tidak bisa menghubungi layanan transkripsi".into(),
        }
    }
}

#[derive(Deserialize)]
struct GroqErrorBody {
    #[serde(default)]
    error: Option<GroqErrorDetail>,
}

#[derive(Deserialize)]
struct GroqErrorDetail {
    #[serde(default)]
    message: Option<String>,
}

/// Normalisasi mime internal → tipe yang diterima Groq (docs/01 §6).
/// Contoh: WAV terdeteksi sebagai `audio/x-wav` (magic bytes) → Groq minta `audio/wav`.
pub fn groq_mime(mime: &str) -> String {
    match mime {
        "audio/x-wav" => "audio/wav".to_string(),
        other => other.to_string(),
    }
}

impl GroqClient {
    pub fn new(
        api_key: String,
        base_url: String,
        max_retries: u32,
        retry_base_ms: u64,
    ) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(300)) // chunk ≤10 menit audio
                .connect_timeout(Duration::from_secs(10))
                .build()?,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            max_retries,
            retry_base_ms,
        })
    }

    /// Klien untuk integration test — tanpa retry delay berarti.
    pub fn for_test(api_key: &str, base_url: &str) -> Result<Self> {
        Self::new(api_key.into(), base_url.into(), 3, 1)
    }

    pub async fn transcribe(
        &self,
        audio: Vec<u8>,
        params: &TranscribeParams<'_>,
    ) -> Result<GroqVerbose, GroqError> {
        let mut attempt: u32 = 0;
        loop {
            let outcome = self.transcribe_once(audio.clone(), params).await;
            match outcome {
                Ok(v) => return Ok(v),
                Err(err) if !err.is_retryable() => return Err(err),
                Err(err) => {
                    if attempt >= self.max_retries {
                        tracing::warn!(attempt, error = %err, "groq: retry habis");
                        return Err(err);
                    }
                    let backoff = self.backoff_delay(&err, attempt);
                    tracing::warn!(attempt, backoff_ms = backoff, error = %err, "groq: retry");
                    tokio::time::sleep(Duration::from_millis(backoff)).await;
                    attempt += 1;
                }
            }
        }
    }

    async fn transcribe_once(
        &self,
        audio: Vec<u8>,
        params: &TranscribeParams<'_>,
    ) -> Result<GroqVerbose, GroqError> {
        let url = format!("{}/audio/transcriptions", self.base_url);

        let file_part = reqwest::multipart::Part::bytes(audio)
            .file_name(params.filename.to_string())
            .mime_str(params.mime)
            .map_err(|e| GroqError::Network(format!("mime tidak valid: {e}")))?;

        let mut form = reqwest::multipart::Form::new()
            .text("model", params.model.to_string())
            .text("response_format", "verbose_json")
            .part("file", file_part);
        if let Some(lang) = params.language {
            form = form.text("language", lang.to_string());
        }
        // Prompt konteks antar chunk: bawa teks chunk sebelumnya (kontinuitas istilah).
        if let Some(prompt) = params.prompt
            && !prompt.is_empty()
        {
            form = form.text("prompt", prompt.to_string());
        }

        let res = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| GroqError::Network(e.to_string()))?;

        let status = res.status().as_u16();
        let retry_after = res
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());

        if res.status().is_success() {
            return res
                .json::<GroqVerbose>()
                .await
                .map_err(|e| GroqError::NonRetryable {
                    status,
                    message: format!("respons tidak valid: {e}"),
                });
        }

        let body = res.text().await.unwrap_or_default();
        let detail = serde_json::from_str::<GroqErrorBody>(&body)
            .ok()
            .and_then(|b| b.error)
            .and_then(|e| e.message)
            .unwrap_or_else(|| body.chars().take(200).collect());

        match status {
            429 | 500 | 502 | 503 | 504 => Err(GroqError::Retryable {
                status,
                message: detail,
                retry_after,
            }),
            code => Err(GroqError::NonRetryable {
                status: code,
                message: detail,
            }),
        }
    }

    fn backoff_delay(&self, err: &GroqError, attempt: u32) -> u64 {
        if let GroqError::Retryable {
            retry_after: Some(s),
            ..
        } = err
        {
            return s.saturating_mul(1000).min(30_000);
        }
        let exp = self.retry_base_ms.saturating_mul(1 << attempt.min(6));
        let jitter = rand::rng().random_range(0..=(exp / 4).max(1));
        (exp + jitter).min(30_000)
    }
}
