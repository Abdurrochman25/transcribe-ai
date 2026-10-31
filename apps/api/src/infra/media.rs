//! Validasi tipe media via magic bytes — ekstensi/content-type klien TIDAK dipercaya
//! (docs/08 §2). Allowlist: format yang didukung Groq Whisper.

/// Return mime kanonik bila prefix dikenali sebagai media yang diizinkan.
pub fn sniff_mime(prefix: &[u8]) -> Option<&'static str> {
    if prefix.len() < 12 {
        return None;
    }
    match () {
        _ if prefix.starts_with(b"ID3") => Some("audio/mpeg"),
        // MP3 raw: frame sync 11 bit (0xFFEx)
        _ if prefix[0] == 0xFF && prefix[1] & 0xE0 == 0xE0 => Some("audio/mpeg"),
        // WAV: "RIFF" .... "WAVE"
        _ if prefix.starts_with(b"RIFF") && &prefix[8..12] == b"WAVE" => Some("audio/x-wav"),
        _ if prefix.starts_with(b"OggS") => Some("audio/ogg"),
        _ if prefix.starts_with(b"fLaC") => Some("audio/flac"),
        // MP4/M4A: bytes 4..8 == "ftyp"
        _ if &prefix[4..8] == b"ftyp" => Some("audio/mp4"),
        // WebM/Matroska
        _ if prefix.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) => Some("video/webm"),
        _ => None,
    }
}

/// Label ramah untuk mime terdeteksi (pesan error/UI).
pub fn mime_label(mime: &str) -> &'static str {
    match mime {
        "audio/mpeg" => "MP3",
        "audio/x-wav" => "WAV",
        "audio/ogg" => "OGG",
        "audio/flac" => "FLAC",
        "audio/mp4" => "M4A/MP4",
        "video/webm" => "WEBM",
        _ => "tidak dikenal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(b: &[u8]) -> Vec<u8> {
        let mut v = b.to_vec();
        v.resize(32, 0);
        v
    }

    #[test]
    fn deteksi_semua_format() {
        assert_eq!(sniff_mime(&pad(b"ID3\x04\x00")).unwrap(), "audio/mpeg");
        assert_eq!(
            sniff_mime(&pad(&[0xFF, 0xFB, 0x90, 0x00])).unwrap(),
            "audio/mpeg"
        );
        let mut wav = b"RIFF\x00\x00\x00\x00WAVE".to_vec();
        wav.extend_from_slice(&[0u8; 24]);
        assert_eq!(sniff_mime(&wav).unwrap(), "audio/x-wav");
        assert_eq!(sniff_mime(&pad(b"OggS\x00\x02")).unwrap(), "audio/ogg");
        assert_eq!(sniff_mime(&pad(b"fLaC\x00\x00")).unwrap(), "audio/flac");
        let mut mp4 = vec![0u8; 32];
        mp4[4..8].copy_from_slice(b"ftyp");
        assert_eq!(sniff_mime(&mp4).unwrap(), "audio/mp4");
        let mut webm = vec![0x1A, 0x45, 0xDF, 0xA3];
        webm.resize(32, 0);
        assert_eq!(sniff_mime(&webm).unwrap(), "video/webm");
    }

    #[test]
    fn file_non_media_ditolak() {
        assert!(sniff_mime(&pad(b"PK\x03\x04 zipp")).is_none(), "zip");
        assert!(sniff_mime(&pad(b"<html><body>")).is_none(), "html");
        assert!(sniff_mime(&pad(b"hello world!")).is_none(), "teks");
        assert!(sniff_mime(b"short").is_none(), "terlalu pendek");
    }
}
