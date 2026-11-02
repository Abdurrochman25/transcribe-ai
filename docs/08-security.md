# 08 — Security & Privacy

## 1. AuthN / AuthZ

| Area | Kebijakan |
|---|---|
| Password | Argon2id (m=19 MiB, t=2, p=1 — OWASP baseline); min 10 char + strength check |
| Access token | JWT HS256, TTL 15 menit, payload hanya `sub`, `iat`, `exp`, `jti` |
| Refresh token | Random 256-bit, **disimpan sebagai SHA-256 hash** di DB, TTL 30 hari, rotasi tiap refresh (revoke lama), reuse-detected → revoke semua sesi user |
| Token storage (web) | Access: memori JS. Refresh: cookie `httpOnly; Secure; SameSite=Lax` |
| Ownership | Semua resource `transcripts` selalu difilter `user_id` di query level — bukan hanya pengecekan setelah fetch |
| Rate limit | auth 10/menit/IP; upload 20/jam/user; umum 60/menit/user (governor) |
| Enumeration | Pesan login gagal generik; register email-exists → respons 200-netral + email konfirmasi (roadmap), MVP: 409 tapi tidak untuk login |

## 2. Keamanan Upload File

1. **Batas ukuran** — ditolak di proxy (200 MB) dan di Axum layer (`RequestBodyLimit`) sebelum buffer.
2. **Magic bytes** — ekstensi tidak dipercaya; deteksi signature (mp3 `ID3`/`0xFFFB`, wav `RIFF`, m4a/mp4 `ftyp`, ogg `OggS`, flac `fLaC`, webm `0x1A45DFA3`).
3. **Allowlist** mime; selainnya 400 tanpa detail internal.
4. **Filename** — disanitasi (tidak dipakai untuk path); `storage_key = users/{user_id}/{uuid}/source`.
5. **ffprobe** — file harus terbaca sebagai media valid; gagal → `failed` job (bukan error 500).
6. **Storage** — bucket private; akses audio hanya via presigned URL ≤15 menit; `Content-Disposition: attachment` untuk export.
7. **Audio tidak pernah diproses oleh pihak ketiga selain Groq** (data processor) — lihat §5.

## 3. Hardening HTTP

- CORS allowlist ketat (`CORS_ALLOWED_ORIGINS`); tanpa wildcard saat kredensial aktif.
- Header: `Strict-Transport-Security`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: strict-origin-when-cross-origin`, `Content-Security-Policy` di frontend (meta) — tanpa inline script.
- Timeout request 60s (upload 600s), body limit, multipart memory-limit 10 MB (sisanya stream ke tempfile).
- Dependency audit: `cargo audit` di CI + Dependabot.
- Secrets hanya via env/secret manager; tidak pernah di repo; `.env` ada di `.gitignore`.

## 4. Keamanan Server & Job

- Worker berjalan sebagai user non-root di container; ffmpeg dijalankan dengan `-nostdin`, timeout proses, dan resource limit (`ulimit`/cgroup) agar file jahat tidak membunuh host.
- Log tidak pernah berisi isi transcript / token / nama file penuh — hanya `transcript_id` + metadata.
- `request_id` di error respons untuk korelasi log tanpa membocorkan stack trace (500 generik).
- Soft delete → purge terjadwal; operasi admin (masa depan) lewat role `admin` + audit log.

## 5. Privasi & Kepatuhan

| Poin | Kebijakan |
|---|---|
| Data dikumpulkan | email, nama, file audio (sementara), hasil transkripsi |
| Retensi audio | default 30 hari setelah selesai (configurable per deployment; user bisa hapus manual kapan pun) |
| Hak penghapusan | DELETE transcript (soft) → purge ≤7 hari; akun: hapus akun = cascade semua data (v1.1) |
| Groq sebagai sub-processor | dicatat di privacy policy; Groq tidak memakai data API untuk training (per dokumentasi mereka) |
| Export data user | `GET /transcripts?format=json` massal (v1.1) — data portability |
| GDPR-friendly | consent banner minimal, DPA tersedia untuk team plan (v2) |
| Log analytics | tidak ada third-party tracker di MVP; telemetry self-hosted opsional |

## 6. Checklist Rilis (MVP)

- [x] Semua endpoint mutasi butuh auth & scoped user
- [x] Rate limit aktif di auth + upload
- [x] Magic-bytes validation + size limit teruji (file 300 MB ditolak 413)
- [x] Refresh rotation + reuse detection teruji
- [x] JWT_SECRET ≥ 32 byte random, di-rotate-able (validasi fail-fast + runbook insiden §5a)
- [x] `cargo audit` + `npm audit` bersih (Rust: 0 aktif — 1 ignore terdokumentasi `.cargo/audit.toml` untuk entri stale; web: hanya 4 low, gate CI critical)
- [x] CSP + HSTS aktif di deployment (CSP prod di `hooks.server.ts`; HSTS otomatis saat `COOKIE_SECURE=true` + rekomendasi Cloudflare)
- [x] Prosedur insiden: cara revoke semua token (rotate JWT_SECRET) terdokumentasi — `docs/11-deploy-runbook.md §5`
