# 10 — Roadmap Pengembangan

Estimasi untuk 1 developer full-time (bisa dipercepat kalau paralel). Setiap fase diakhiri demo yang bisa dipakai.

## Fase 0 — Fondasi (minggu 1)
- [x] Scaffold monorepo: `apps/api` (cargo), `apps/web` (SvelteKit), docker-compose dev
- [x] CI GitHub Actions: fmt + clippy + test (api), check + lint + build (web)
- [x] Migrasi awal (0001_init), config loader, tracing, error envelope, health/ready
- [x] Konvensi: commit message (conventional commits), branch naming, code review checklist
- [ ] Demo: `docker compose up` → `/health` hijau *(butuh Docker Desktop di mesin lokal; CI & file compose siap)*
- **Demo:** `docker compose up` → `/health` hijau, hello-world auth endpoint.

## Fase 1 — Auth & User (minggu 2)
- [x] Register/login/refresh/logout/me + Argon2 + JWT + rotasi refresh
- [x] Middleware auth + rate limit + error 401/403 yang benar
- [x] Frontend: halaman login/register, session store, guard route `(app)`
- **Demo:** daftar → masuk → dashboard kosong (protected).

## Fase 2 — Upload & Storage (minggu 3)
- [x] POST /transcripts multipart streaming → S3/MinIO + magic-byte validation + 413
- [x] ffprobe wrapper (durasi/mime), simpan metadata
- [x] Frontend: FileDropzone + wizard opsi + progress upload byte
- **Demo:** upload mp3 muncul di dashboard dengan metadata durasi.

## Fase 3 — Job Pipeline & Groq (minggu 4–5)
- [ ] Worker supervisor (claim SKIP LOCKED), state machine, SSE hub
- [ ] GroqClient + retry/backoff, verbose_json → segments (offset chunking)
- [ ] Chunking ffmpeg untuk file >25 MB + checkpoint cancel
- [ ] Reaper job stale + maintenance task retensi
- [ ] Frontend: progress live, badge status, polling fallback
- [ ] Integration test dengan wiremock (happy + 429 retry + cancel)
- **Demo:** file 40 menit → transcript selesai dengan progress live. *(Kriteria metrik: <60 dtk untuk audio 10 menit.)*

## Fase 4 — Editor & Export (minggu 6–7)
- [ ] GET /transcripts/:id (+presigned audio), PATCH segments/title
- [ ] Export TXT/SRT/VTT/JSON + unit test formatter
- [ ] Frontend: player sinkron, SegmentList virtualized, edit inline optimistic, ExportMenu
- [ ] Retry & cancel dari UI
- **Demo:** alur penuh upload→edit→unduh SRT. **← MVP FEATURE COMPLETE**

## Fase 5 — Hardening Rilis (minggu 8)
- [ ] Security checklist (dok 08) penuh; cargo audit; CSP; HSTS
- [ ] Deploy staging (Fly/VPS + R2) + domain + Cloudflare
- [ ] UAT kecil (5 pengguna), perbaikan UX dari feedback
- [ ] Landing page publik + privacy policy + ToS
- [ ] Beta publik (invite code) + monitoring & alert aktif
- **Milestone: Rilis Beta 🚀**

## Fase 6 — v1.1 (minggu 9–12, post-beta)
- [ ] LISTEN/NOTIFY antrean (latensi job turun)
- [ ] pg_trgm search + filter lanjutan; hapus akun + purge
- [ ] Hapus semua sesi; tema gelap; i18n persiapan EN
- [ ] Metrics Prometheus + dashboard Grafana
- [ ] Optimistic chunk di boundary silence (akurasi sambungan naik)

## Fase 7 — v2 (kuartal berikut)
- [ ] Speaker diarization (service terpisah, kandidat: pyannote via sidecar)
- [ ] Ringkasan & keywords via Groq LLM (llama-4) — endpoint `/transcripts/:id/summary`
- [ ] Terjemahan subtitle; share link publik read-only
- [ ] Team workspace, kuota & billing (Stripe), API keys + webhooks
- [ ] Streaming realtime (WebSocket, Web Speech/Whisper streaming)

## Definisi Selesai per Fase

1. Semua item checklist tercentang.
2. CI hijau (fmt/clippy/test; check/lint/test/build web).
3. Demo fase berjalan di lingkungan dev (`compose up`).
4. PR direview (self-review minimal) + squash ke `main`.
5. Catatan singkat "what changed" di PR description.

## Risiko & Mitigasi

| Risiko | Dampak | Mitigasi |
|---|---|---|
| Rate limit Groq di traffic tinggi | job antre lama | antrean DB + backoff; tier upgrade; option model turbo |
| Chunking memotong kata | kualitas transkripsi turun | potong di silence (v1.1); overlap 1 dtk; prompt hint konteks chunk |
| ffprobe crash pada file aneh | worker mati | subprocess timeout + resource limit; job failed rapi |
| SaaS frontend deploy ribet di SSR | lambat rilis | adapter-node di Docker — sama dengan API, satu pola deploy |
| Scope creep (diarization, dll. masuk MVP) | telat | MoSCoW di dok 01 dipakai sebagai kontrak scope |
