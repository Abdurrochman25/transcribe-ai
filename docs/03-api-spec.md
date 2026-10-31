# 03 — API Specification (REST `/api/v1`)

Konvensi umum:
- Base URL: `https://<host>/api/v1`
- Format: JSON (`application/json`), kecuali upload (multipart) & SSE (event-stream).
- Autentikasi: `Authorization: Bearer <access_token>` — ditandai 🔒.
- Timestamp: ISO-8601 UTC (`2026-10-31T09:00:00Z`).
- ID: UUID v4/v7.

## Format Error (konsisten, semua endpoint)

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Request tidak valid",
    "details": [
      { "field": "file", "issue": "tipe file tidak didukung" }
    ],
    "request_id": "01JD7Z3F9GKQ..."
  }
}
```

| Kode error aplikasi | HTTP | Kapan |
|---|---|---|
| `VALIDATION_ERROR` | 400 | payload tidak valid |
| `UNAUTHENTICATED` | 401 | token hilang/kadaluarsa |
| `FORBIDDEN` | 403 | bukan pemilik resource |
| `NOT_FOUND` | 404 | resource tidak ada / sudah dihapus |
| `CONFLICT` | 409 | konflik state (mis. cancel job yang sudah done) |
| `INVALID_STATE_TRANSITION` | 409 | aksi tidak sesuai state machine |
| `PAYLOAD_TOO_LARGE` | 413 | melebihi `MAX_UPLOAD_BYTES` |
| `RATE_LIMITED` | 429 | kena limit (lihat header) |
| `INTERNAL` | 500 | error tak terduga |

## Rate Limiting

Header respons semua endpoint:
```
X-RateLimit-Limit: 60
X-RateLimit-Remaining: 59
X-RateLimit-Reset: 1761900000
Retry-After: 30        # hanya saat 429
```
Default (configurable): auth 10/menit/IP; upload 20/jam/user; API umum 60/menit/user.

---

## Auth

### POST `/auth/register`
```json
{ "email": "andi@example.com", "password": "S3cret!234", "name": "Andi" }
```
**201**
```json
{
  "user": { "id": "uuid", "email": "andi@example.com", "name": "Andi", "plan": "free", "created_at": "..." },
  "tokens": { "access_token": "eyJ...", "refresh_token": "rt_...", "expires_in": 900 }
}
```
Aturan: email valid & unik; password min 10 char (dicek [zxcvbn-style strength], blok `password123`-class).

### POST `/auth/login`
```json
{ "email": "andi@example.com", "password": "S3cret!234" }
```
**200** → sama dengan respons register. **401** dengan pesan generik (anti user-enumeration).
Refresh token juga di-set sebagai cookie `httpOnly; Secure; SameSite=Lax; Path=/api/v1/auth`.

### POST `/auth/refresh` 🔒 (cookie refresh token)
**200** → `{ "tokens": { ... } }` — rotasi: refresh lama dicabut, token baru diterbitkan.

### POST `/auth/logout` 🔒
**204** — cabut refresh token aktif (cookie dibersihkan).

### GET `/auth/me` 🔒
**200** → `{ "user": { ... } }`

---

## Transcripts

### POST `/transcripts` 🔒 — upload & mulai job
`multipart/form-data`:

| Field | Tipe | Wajib | Keterangan |
|---|---|---|---|
| `file` | binary | ✅ | audio/video; divalidasi magic bytes; maks `MAX_UPLOAD_BYTES` (default 200 MB) |
| `title` | string | ❌ | default = nama file |
| `language` | string | ❌ | ISO-639-1 (`id`, `en`…); kosong = auto-detect |
| `model` | string | ❌ | `whisper-large-v3-turbo` (default) \| `whisper-large-v3` |
| `prompt` | string | ❌ | prompt hint ≤ 500 char (istilah khusus, nama, singkatan) |

**202**
```json
{
  "transcript": {
    "id": "0192...",
    "title": "Kuliah AI pertemuan 3",
    "status": "queued",
    "progress": 0,
    "source_filename": "kuliah.m4a",
    "size_bytes": 15204352,
    "language": null,
    "requested_language": "id",
    "model": "whisper-large-v3-turbo",
    "duration_seconds": null,
    "created_at": "2026-10-31T09:00:00Z"
  }
}
```

### GET `/transcripts` 🔒 — list milik user
Query: `page` (default 1), `per_page` (default 20, maks 100), `status` (`completed|queued|processing|failed`), `q` (cari di title), `sort` (`-created_at` default).

**200**
```json
{
  "items": [ { "transcript": {...ringkas...} } ],
  "page": 1, "per_page": 20,
  "total": 42,
  "total_pages": 3
}
```

### GET `/transcripts/:id` 🔒
**200** — detail + segments (dipakai editor):
```json
{
  "transcript": {
    "id": "0192...", "title": "...", "status": "completed", "progress": 100,
    "language": "id", "detected_language_probability": 0.98,
    "duration_seconds": 742.3, "error": null, "created_at": "...", "completed_at": "..."
  },
  "segments": [
    { "id": 1, "idx": 0, "start_ms": 0, "end_ms": 4210, "text": "Selamat pagi, hari ini kita...", "speaker": null, "edited": false }
  ],
  "audio_url": "https://s3.../presigned..."
}
```

### GET `/transcripts/:id/events` 🔒 — SSE progress
```
event: progress
data: {"status":"transcribing","progress":42,"message":"chunk 2/5"}

event: completed
data: {"transcript_id":"0192...","duration_seconds":742.3}

event: failed
data: {"message":"Gagal setelah 3x percobaan: upstream 500"}

event: heartbeat (tiap 15s, comment frame)
```

### PATCH `/transcripts/:id` 🔒 — edit metadata/segments
```json
{ "title": "Kuliah AI #3 — LLM", "segments": [ { "id": 1, "text": "Selamat pagi Bapak/Ibu, hari ini..." } ] }
```
**200** → detail terbaru. Segment yang diubah ditandai `edited=true` (jejak edit untuk audit ringan). Edit segment hanya saat `status=completed` → selain itu **409**.

### POST `/transcripts/:id/retry` 🔒 — job gagal → antre ulang
**202** `{ "transcript": { "id": "...", "status": "queued" } }` — hanya dari `failed`, selain itu **409**.

### POST `/transcripts/:id/cancel` 🔒
**202** → set `cancel_requested`; worker stop di checkpoint. Sudah final → **409**.

### DELETE `/transcripts/:id` 🔒 — soft delete
**204**. Audio dihapus dari storage oleh reaper async; `GET` setelahnya → **404**.

### GET `/transcripts/:id/export?format=txt|srt|vtt|json` 🔒
**200** dengan `Content-Disposition: attachment; filename="judul.srt"`.

Format SRT:
```
1
00:00:00,000 --> 00:00:04,210
Selamat pagi, hari ini kita...

2
00:00:04,210 --> 00:00:09,870
...
```
VTT pakai `WEBVTT` header + titik desimal. JSON = representasi penuh (segments + metadata). TXT = teks gabungan (paragraf per menit).

---

## Utilities

### GET `/health` (publik) — liveness
**200** `{ "status": "ok", "version": "0.1.0" }`

### GET `/ready` (publik) — readiness: cek DB & storage
**200** / **503** `{ "db": true, "storage": false }`

### GET `/languages` 🔒 — daftar bahasa yang didukung (kode + label)
Cacheable ETag.

---

## Kontrak Versi & Kompatibilitas

- Breaking change → `/api/v2`; `/api/v1` dijaga minimal 6 bulan setelah v2 rilis.
- Field baru boleh ditambah (klien harus ignore-unknown).
- Deprecation ditandai header `Deprecation` + `Sunset`.
