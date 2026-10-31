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
