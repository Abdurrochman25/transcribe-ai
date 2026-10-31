//! Formatter export transcript — docs/03 §export.
//! TXT (paragraf per menit), SRT (koma milidetik), VTT (WEBVTT + titik), JSON penuh.

use serde::Serialize;
use serde_json::json;

use crate::domain::transcript::{TranscriptPublic, TranscriptRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Txt,
    Srt,
    Vtt,
    Json,
}

impl ExportFormat {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "txt" => Self::Txt,
            "srt" => Self::Srt,
            "vtt" => Self::Vtt,
            "json" => Self::Json,
            _ => return None,
        })
    }

    pub fn ext(self) -> &'static str {
        match self {
            Self::Txt => "txt",
            Self::Srt => "srt",
            Self::Vtt => "vtt",
            Self::Json => "json",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Self::Txt => "text/plain",
            Self::Srt => "application/x-subrip",
            Self::Vtt => "text/vtt",
            Self::Json => "application/json",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ExportSegment {
    pub idx: i32,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub confidence: Option<f32>,
    pub edited: bool,
}

/// Nama file aman dari judul: alphanumeric + tanda hubung (slug ringan).
pub fn slugify(title: &str) -> String {
    let slug: String = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = slug.trim_matches('-').to_string();
    let collapsed: String = trimmed
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if collapsed.is_empty() {
        "transkripsi".into()
    } else {
        collapsed.chars().take(80).collect()
    }
}

fn tc_srt(ms: i64) -> String {
    let h = ms / 3_600_000;
    let m = (ms / 60_000) % 60;
    let s = (ms / 1000) % 60;
    let frac = ms % 1000;
    format!("{h:02}:{m:02}:{s:02},{frac:03}")
}

fn tc_vtt(ms: i64) -> String {
    tc_srt(ms).replace(',', ".")
}

/// TXT: teks gabungan, paragraf baru tiap berganti menit (docs/03).
pub fn to_txt(segments: &[ExportSegment]) -> String {
    let mut out = String::new();
    let mut current_minute: i64 = -1;
    for s in segments {
        let minute = s.start_ms / 60_000;
        if minute != current_minute {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            current_minute = minute;
        } else if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(s.text.trim());
    }
    out
}

pub fn to_srt(segments: &[ExportSegment]) -> String {
    let mut out = String::new();
    for (i, s) in segments.iter().enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            tc_srt(s.start_ms),
            tc_srt(s.end_ms),
            s.text.trim()
        ));
    }
    out
}

pub fn to_vtt(segments: &[ExportSegment]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for s in segments {
        out.push_str(&format!(
            "{} --> {}\n{}\n\n",
            tc_vtt(s.start_ms),
            tc_vtt(s.end_ms),
            s.text.trim()
        ));
    }
    out
}

pub fn to_json(transcript: &TranscriptPublic, segments: &[ExportSegment]) -> String {
    let body = json!({
        "transcript": transcript,
        "segments": segments,
    });
    serde_json::to_string_pretty(&body).unwrap_or_else(|_| "{}".into())
}

/// Render sesuai format — dipakai handler export & unit test.
pub fn render(
    format: ExportFormat,
    transcript: &TranscriptRow,
    segments: &[ExportSegment],
) -> String {
    match format {
        ExportFormat::Txt => to_txt(segments),
        ExportFormat::Srt => to_srt(segments),
        ExportFormat::Vtt => to_vtt(segments),
        ExportFormat::Json => to_json(&transcript.to_public(), segments),
    }
}

/// Alias untuk handler: dari SegmentRow repo → ExportSegment.
pub fn from_repo(rows: &[crate::repositories::transcripts::SegmentRow]) -> Vec<ExportSegment> {
    rows.iter()
        .map(|r| ExportSegment {
            idx: r.idx,
            start_ms: r.start_ms,
            end_ms: r.end_ms,
            text: r.text.clone(),
            confidence: r.confidence,
            edited: r.edited,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn seg(idx: i32, start_ms: i64, end_ms: i64, text: &str) -> ExportSegment {
        ExportSegment {
            idx,
            start_ms,
            end_ms,
            text: text.into(),
            confidence: None,
            edited: false,
        }
    }

    #[test]
    fn srt_format_sesuai_spec() {
        let out = to_srt(&[
            seg(0, 0, 4210, "Selamat pagi."),
            seg(1, 4210, 9870, "Perkenalkan topik."),
        ]);
        assert_eq!(
            out,
            "1\n00:00:00,000 --> 00:00:04,210\nSelamat pagi.\n\n\
             2\n00:00:04,210 --> 00:00:09,870\nPerkenalkan topik.\n\n"
        );
    }

    #[test]
    fn vtt_header_dan_desimal() {
        let out = to_vtt(&[seg(0, 61_500, 65_000, "Satu menit.")]);
        assert!(out.starts_with("WEBVTT\n\n"));
        assert!(out.contains("00:01:01.500 --> 00:01:05.000\nSatu menit."));
    }

    #[test]
    fn txt_paragraf_per_menit() {
        let out = to_txt(&[
            seg(0, 0, 4000, "Awal menit pertama."),
            seg(1, 30_000, 34_000, "Masih menit pertama."),
            seg(2, 61_000, 64_000, "Menit kedua."),
        ]);
        assert_eq!(
            out,
            "Awal menit pertama. Masih menit pertama.\n\nMenit kedua."
        );
    }

    #[test]
    fn slugify_judul() {
        assert_eq!(slugify("Kuliah AI #3 — LLM!"), "kuliah-ai-3-llm");
        assert_eq!(slugify("///---///"), "transkripsi");
    }

    #[test]
    fn format_parse_dan_mime() {
        assert_eq!(ExportFormat::parse("SRT"), Some(ExportFormat::Srt));
        assert_eq!(ExportFormat::parse("mp4"), None);
        assert_eq!(ExportFormat::Vtt.mime(), "text/vtt");
    }

    #[test]
    fn json_berisi_transcript_dan_segments() {
        // Cukup pastikan JSON valid & memuat kunci utama (tanpa TranscriptRow penuh).
        let val: Value = serde_json::from_str(&to_json(&dummy_public(), &[seg(0, 0, 10, "Halo.")]))
            .expect("json valid");
        assert!(val["transcript"]["id"].is_string());
        assert_eq!(val["segments"][0]["text"], "Halo.");
    }

    fn dummy_public() -> TranscriptPublic {
        TranscriptPublic {
            id: uuid::Uuid::now_v7(),
            title: "T".into(),
            status: "completed".into(),
            progress: 100,
            source_filename: "a.wav".into(),
            size_bytes: 1,
            mime_type: "audio/x-wav".into(),
            duration_seconds: None,
            language: None,
            requested_language: None,
            model: "m".into(),
            prompt_hint: None,
            error: None,
            created_at: time::OffsetDateTime::now_utc(),
            completed_at: None,
        }
    }
}
