# 01 — Overview Produk

## 1. Visi

Menjadi cara tercepat dan termudah mengubah audio menjadi teks akurat yang siap dipakai — untuk mahasiswa yang mentranskrip kuliah, jurnalis yang mewawancarai, sampai tim yang butuh notulen meeting.

**Positioning:** kecepatan (Groq = inference sangat cepat), kesederhanaan (upload → teks dalam hitungan detik/menit), harga transparan.

## 2. Problem Statement

| Persona | Pain hari ini | Solusi TranscribeAI |
|---|---|---|
| **Mahasiswa** (Andi, 21) | Mentranskrip rekaman kuliah 2 jam manual butuh ±6 jam | Upload → transcript siap dalam hitungan menit, export TXT |
| **Jurnalis** (Bunga, 29) | Dari aplikasi lain tidak bisa perbaiki salah ketik nama/istilah | Editor inline dengan timestamp + prompt hint untuk istilah khusus |
| **Tim remote** (PT Maju) | Notulen meeting manual, tidak konsisten | Upload rekaman meeting → ringkasan (v2) + subtitle SRT untuk arsip video |
| **Developer** (Rafi, 26) | Butuh transkrip programatis untuk produk sendiri | API publik + API key (v2) |

## 3. Fitur & Prioritas (MoSCoW)

### MUST (MVP)
- Auth email/password (register, login, refresh, logout)
- Upload audio/video via drag & drop + validasi tipe & ukuran
- Deteksi durasi & metadata via ffprobe
- Transkripsi async via Groq Whisper (`whisper-large-v3-turbo` default)
- Progress realtime (SSE): status + persentase
- Auto-detect bahasa + pilihan manual + prompt hint
- Editor transcript bertimestamp (edit inline, klik-segment → lompat audio)
- Export TXT / SRT / VTT / JSON
- Riwayat transcript: list, cari, filter status, hapus (soft delete)
- Chunking file >25 MB via ffmpeg dengan penggabungan timestamp
- Rate limiting per-user

### SHOULD (v1.1)
- Hard delete + purge storage (kepatuhan)
- Retry job gagal dari UI
- Dark/light mode
- Pinned/favorite transcript
- Copy-to-clipboard per segment

### COULD (v2)
- Speaker diarization
- Ringkasan & keywords via LLM
- Terjemahan subtitle
- Team workspace & share link
- API publik + API keys + webhooks
- Upload dari URL (YouTube link, dll.)

### WON'T (untuk sekarang)
- Mobile native app
- On-device / offline transcription
- Video editing

## 4. Metrik Sukses

| Metrik | Target MVP | Cara ukur |
|---|---|---|
| Time-to-transcript (upload→selesai, file 10 menit) | < 60 detik | telemetry job duration |
| Transcription job success rate | > 98% | job log |
| Upload success rate | > 99% | access log |
| cold p95 API latency (non-transkripsi) | < 150 ms | tracing |
| Hari-1 user langsung menghasilkan 1 transcript | > 70% | event analytics (privacy-aware) |

## 5. Non-Goals (eksplisit)

- Bukan layanan penyimpanan file jangka panjang — audio dihapus otomatis setelah X hari (default 30, konfigurable).
- Bukan editor audio — hanya editor teks.
- Tidak menjanjikan akurasi 100% — AI; sediakan editor sebagai jawaban.

## 6. Batasan Teknis Eksternal (Groq)

| Batasan | Nilai | Dampak & Mitigasi |
|---|---|---|
| Ukuran file per request | 25 MB (free tier) / 100 MB (dev+ tier) | Chunking via ffmpeg di sisi server |
| Durasi audio maksimum per request | ±25 menit / 7 jam (tier) | Chunk 10 menit default |
| Rate limit API | tergantung tier (TPM) | Retry + exponential backoff + antrean |
| Format yang didukung | flac, mp3, mp4, mpeg, mpga, m4a, ogg, opus, wav, webm | Konversi otomatis via ffmpeg bila perlu |
| Harga | ±$0.04/jam audio untuk `whisper-large-v3-turbo` (cek [harga terbaru](https://console.groq.com/docs/pricing)) | Kuota per plan |

> Angka batasan bisa berubah — implementasi harus membaca nilai ini dari config, bukan hard-code.
