# ADR-002: Engine transkripsi — Groq Whisper API

**Status:** Accepted · **Tanggal:** 2026-10-31

## Konteks
Butuh STT berkualitas Whisper-class, cepat, biaya rendah, tanpa mengelola GPU.

## Alternatif
| Opsi | Plus | Minus |
|---|---|---|
| **Groq Whisper API** ✅ | LPU Groq = latency sangat rendah; OpenAI-compatible API (whisper-large-v3, -turbo); murah; OpenAI-compatible → mudah ganti provider | Batas ukuran file 25–100 MB per tier → butuh chunking; provider muda |
| OpenAI Whisper API | standar de-facto | lebih lambat & mahal per menit |
| AssemblyAI / Deepgram | fitur diarization bawaan | vendor lock-in fitur; biaya lebih tinggi |
| Self-host faster-whisper | gratis, privat | butuh GPU ops, kompleks; tak sejalan " cepat rilis" |

## Keputusan
Groq `whisper-large-v3-turbo` sebagai default; `whisper-large-v3` opsi. Akses via `GROQ_BASE_URL` (OpenAI-compatible) sehingga **bisa dialihkan ke provider OpenAI-compatible lain tanpa perubahan kode** (mitigasi lock-in).

## Konsekuensi
(+) Time-to-transcript memenuhi target metrik (<60 dtk utk 10 mnt audio).
(−) Kompleksitas chunking + offset timestamp jadi tanggung jawab kita (di-desain di dok 02/05).
