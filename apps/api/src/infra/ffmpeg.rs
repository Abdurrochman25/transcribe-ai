//! Wrapper ffprobe — baca durasi & format dari file lokal (docs/05 §infra).

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct ProbeInfo {
    pub duration_seconds: Option<f64>,
    pub format_name: Option<String>,
}

#[derive(Deserialize)]
struct FfprobeOutput {
    #[serde(default)]
    format: Option<Format>,
}

#[derive(Deserialize)]
struct Format {
    duration: Option<String>,
    #[serde(default)]
    format_name: Option<String>,
}

pub async fn probe(ffprobe_path: &str, file: &Path) -> Result<ProbeInfo> {
    let output = Command::new(ffprobe_path)
        .arg("-v")
        .arg("error")
        .arg("-print_format")
        .arg("json")
        .arg("-show_format")
        .arg(file)
        .output()
        .await
        .with_context(|| format!("menjalankan {ffprobe_path} (terpasang di PATH?)"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("ffprobe gagal: {stderr}");
    }

    let parsed: FfprobeOutput =
        serde_json::from_slice(&output.stdout).context("parse output ffprobe")?;
    let Some(format) = parsed.format else {
        bail!("ffprobe tidak menemukan stream format");
    };

    Ok(ProbeInfo {
        duration_seconds: format.duration.and_then(|d| d.parse().ok()),
        format_name: format.format_name,
    })
}

/// Potong audio menjadi segmen `segment_seconds` detik tanpa re-encode (docs/02 §5).
/// Return path chunk terurut. Ekstensi output mengikuti mime agar container cocok.
pub async fn split(
    ffmpeg_path: &str,
    input: &Path,
    segment_seconds: u64,
    out_dir: &Path,
    mime: &str,
) -> Result<Vec<std::path::PathBuf>> {
    let ext = ext_for_mime(mime)?;
    let pattern = out_dir.join(format!("chunk-%03d.{ext}"));

    let output = Command::new(ffmpeg_path)
        .arg("-y")
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(input)
        .arg("-f")
        .arg("segment")
        .arg("-segment_time")
        .arg(segment_seconds.to_string())
        .arg("-c")
        .arg("copy")
        .arg("-reset_timestamps")
        .arg("1")
        .arg(&pattern)
        .output()
        .await
        .with_context(|| format!("menjalankan {ffmpeg_path}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("ffmpeg segment gagal: {stderr}");
    }

    let mut chunks: Vec<_> = std::fs::read_dir(out_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e == ext)
                && p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("chunk-"))
        })
        .collect();
    chunks.sort();
    if chunks.is_empty() {
        bail!("ffmpeg tidak menghasilkan chunk");
    }
    Ok(chunks)
}

/// Ekstensi file untuk mime — publik: dipakai worker untuk nama chunk multipart.
pub fn ext_for_mime(mime: &str) -> anyhow::Result<&'static str> {
    Ok(match mime {
        "audio/mpeg" => "mp3",
        "audio/x-wav" => "wav",
        "audio/ogg" => "ogg",
        "audio/flac" => "flac",
        "audio/mp4" => "m4a",
        "video/webm" => "webm",
        other => bail!("mime tidak dikenal untuk chunking: {other}"),
    })
}
