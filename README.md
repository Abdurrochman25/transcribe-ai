# 🎙️ TranscribeAI

Aplikasi web transkripsi audio/video berbasis AI — ubah rekaman menjadi teks yang akurat, bisa diedit, dan diekspor ke berbagai format (TXT, SRT, VTT, JSON).

> **Status: Fase 0 selesai** — blueprint lengkap di `docs/` + scaffold hidup: API Rust/Axum (health, error envelope, migrasi DB) + web SvelteKit + docker-compose + CI. Lihat [Roadmap](docs/10-roadmap.md).

## Ringkasan Produk

| Aspek | Keputusan |
|---|---|
| **Backend** | Rust (Axum + Tokio + SQLx) — REST API `/api/v1` |
| **Frontend** | SvelteKit 2 (Svelte 5) + TypeScript + Tailwind CSS 4 |
| **AI Engine** | [Groq Whisper API](https://console.groq.com) (`whisper-large-v3` / `whisper-large-v3-turbo`) |
| **Database** | PostgreSQL 16 (prod) / SQLite (dev cepat) |
| **Object Storage** | S3-compatible (MinIO di dev, S3/R2 di prod) |
| **Realtime Progress** | Server-Sent Events (SSE) |
| **Auth** | Email/password (Argon2) + JWT access/refresh token |
| **Deployment** | Docker + docker-compose; target prod: VPS/Fly.io + Cloudflare |

## Fitur Utama (MVP)

1. 🔐 **Auth** — register, login, refresh token, logout
2. 📤 **Upload** — drag & drop multi-format (mp3, wav, m4a, ogg, flac, webm, mp4) dengan validasi & probe durasi
3. ⚡ **Transkripsi async** — job queue di background, progress realtime via SSE, retry otomatis
4. 🌍 **Multi-bahasa** — auto-detect (99 bahasa) atau pilih manual + `prompt hint`
5. ✏️ **Editor transcript** — segment bertimestamp, klik untuk lompat audio, edit inline
6. 📤 **Export** — TXT, SRT, VTT, JSON
7. 🗂️ **Riwayat** — dashboard daftar transcript, filter, pencarian, hapus
8. 🐘 **Chunking otomatis** — file besar dipecah via ffmpeg, timestamp digabung ulang

## Fitur Lanjutan (v2+)

- Speaker diarization (siapa bicara kapan)
- Ringkasan & poin penting via LLM (Groq `llama-4`)
- Terjemahan subtitle multi-bahasa
- Team workspace + share link publik
- API publik + API keys untuk developer
- Transkripsi realtime (streaming WebSocket)
- Webhook saat job selesai

## 📚 Dokumentasi Plan

| # | Dokumen | Isi |
|---|---|---|
| 01 | [Overview](docs/01-overview.md) | Visi, persona, fitur, metrik sukses, scope |
| 02 | [System Architecture](docs/02-architecture.md) | Diagram sistem, komponen, alur data, job pipeline, scaling |
| 03 | [API Specification](docs/03-api-spec.md) | Semua endpoint REST + format error, pagination, SSE, contoh request/response |
| 04 | [Database Design](docs/04-database-design.md) | ERD, tabel, index, strategi migrasi |
| 05 | [Backend Design](docs/05-backend-design.md) | Struktur crate Rust, dependensi, state machine job, integrasi Groq |
| 06 | [Frontend Design](docs/06-frontend-design.md) | Struktur SvelteKit, routing, state management, kontrak API client |
| 07 | [UI/UX Design](docs/07-ui-design.md) | Wireframe, design tokens, user flow, state & aksesibilitas |
| 08 | [Security](docs/08-security.md) | AuthN/AuthZ, validasi file, rate limiting, privasi data |
| 09 | [Deployment & Infra](docs/09-deployment.md) | Docker, CI/CD GitHub Actions, environment, monitoring |
| 10 | [Roadmap](docs/10-roadmap.md) | Fase pengembangan, estimasi, milestone |
| ADR | [Architecture Decision Records](docs/adr/) | Log keputusan: kenapa Axum, Groq, SvelteKit, dll. |

## 🚀 Menjalankan Dev

**Cara cepat (semua service, butuh Docker):**
```bash
cp .env.example .env
docker compose up --build
# web → http://localhost:5173 · API → http://localhost:8080/health
```

**Cara manual (tanpa Docker, perlu Postgres lokal):**
```bash
# 1) API
cd apps/api
DATABASE_URL=postgres://... JWT_SECRET=... cargo run
# 2) Web (terminal lain)
cd apps/web
npm install && npm run dev
```

**Verifikasi kualitas (sama dengan CI):**
```bash
cd apps/api  && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test
cd apps/web  && npm run check && npm run build
```

## Struktur Repo (target saat implementasi)

```
transcribe-ai/
├── apps/
│   ├── api/          # Backend Rust (Axum)
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── config.rs
│   │   │   ├── routes/        # handler per resource
│   │   │   ├── services/      # business logic
│   │   │   ├── repositories/  # akses DB (SQLx)
│   │   │   ├── domain/        # model + state machine
│   │   │   ├── workers/       # job transkripsi async
│   │   │   └── infra/         # storage, groq client, sse hub
│   │   ├── migrations/
│   │   └── Cargo.toml
│   └── web/          # Frontend SvelteKit
│       ├── src/
│       │   ├── routes/
│       │   ├── lib/components/
│       │   ├── lib/api/       # typed API client
│       │   └── lib/stores/
│       ├── package.json
│       └── svelte.config.js
├── docs/             # dokumentasi plan (repo ini)
├── docker-compose.yml
├── Dockerfile.api
├── Dockerfile.web
└── README.md
```

## Lisensi

MIT — lihat [LICENSE](LICENSE).
