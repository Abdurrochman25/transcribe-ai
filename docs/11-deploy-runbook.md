# 11 — Deploy Runbook & Prosedur Insiden (Fase 5)

Lengkapkan dengan docs/09-deployment.md (topologi & env matrix).

## 1. Prasyarat (sekali)

| Kebutuhan | Keterangan |
|---|---|
| Akun Fly.io + CLI | `fly auth login` — https://fly.io |
| Cloudflare R2 bucket | mis. `transcribe-audio` — buat API token (read/write object) |
| Groq API key | https://console.groq.com/keys |
| Domain | dua subdomain: `app.` (web) dan `api.` (API) |

## 2. Deploy pertama (Fly.io + R2)

```powershell
# dari root repo
cd apps/api
fly launch --no-deploy --name transcribe-ai-api --region sin --dockerfile ../Dockerfile.api
fly secrets set `
  DATABASE_URL="postgres://…:…@…/transcribe" `   # Fly Postgres: `fly postgres create`
  JWT_SECRET="<openssl rand -base64 48>" `
  GROQ_API_KEY="gsk_…" `
  CORS_ALLOWED_ORIGINS="https://app.transcribeai.example.com" `
  COOKIE_SECURE="true" `
  S3_ENDPOINT="https://<accountid>.r2.cloudflarestorage.com" `
  S3_BUCKET="transcribe-audio" `
  S3_ACCESS_KEY="…" S3_SECRET_KEY="…" `
  BETA_INVITE_CODE="BETA-2026"          # hapus bila beta terbuka
fly deploy

cd ../web
fly launch --no-deploy --name transcribe-ai-web --region sin --dockerfile ../Dockerfile.web
fly secrets set ORIGIN="https://app.transcribeai.example.com"
# PUBLIC_API_URL di-set build-time: perhatikan apps/web/fly.toml
fly deploy
```

Verifikasi: `curl https://api.…/health` → `{"status":"ok"}`; buka `https://app…/` → landing.

## 3. DNS + Cloudflare

1. A/AAAA (proxy aktif) `app` → web machine, `api` → api machine.
2. SSL/TLS mode **Full (strict)**.
3. Aktifkan **HSTS** di Cloudflare (atau biarkan API yang kirim — otomatis saat `COOKIE_SECURE=true`).
4. Cache rule: bypass untuk `/api/*`.

## 4. Monitoring & alert (MVP)

- **Uptime**: Better Stack / UptimeRobot ping `https://api…/ready` tiap 1 menit (readiness = DB + storage).
- **Queue depth** (SQL manual / panel PG):
  ```sql
  SELECT count(*) FROM transcripts WHERE status IN ('queued','validating','chunking','transcribing','processing');
  ```
  Alert bila > 50 selama 15 menit.
- Log stdout JSON sudah terkumpul di Fly Logs (retensi 30 hari? sesuaikan plan).

## 5. Prosedur Insiden

### a. Mencabut SEMUA sesi (akun/token bocor)
1. `fly secrets set -a transcribe-ai-api JWT_SECRET="<baru>"` → deploy restart otomatis.
2. Semua access token invalid (signature mismatch); refresh token lama tak terpakai —
   opsional: `DELETE FROM refresh_tokens WHERE user_id='<uuid>'` untuk user spesifik.

### b. Groq bermasalah (5xx massal / rate limit)
- Worker sudah auto-retry backoff + hormati `Retry-After` (docs/02 §4).
- Bila > 30 menit: `fly secrets set -a transcribe-ai-api WORKER_CONCURRENCY=0` (worker mati,
  upload tetap aman), pulihkan nilai setelah Groq normal. Job stale di-antre ulang reaper.

### c. DB penuh / lambat
1. Bersihkan event lama: `DELETE FROM transcript_events WHERE created_at < now() - interval '30 days';`
2. Purge soft-deleted: `DELETE FROM transcripts WHERE deleted_at < now() - interval '7 days';`
3. Bila darurat: `pg_dump` dulu (lihat §6).

### d. Audio bocor / salah bagikan
- Retensi 30 hari otomatis; darurat: `UPDATE transcripts SET storage_key='' WHERE id='<uuid>';`
  lalu hapus object di R2 console.

## 6. Backup

| Komponen | Frekuensi | Retensi |
|---|---|---|
| Fly Postgres PITR | kontinu (plan sesuai) | 7 hari |
| `pg_dump` mingguan ke R2 `backups/` | mingguan | 4 snapshot |
| Audio R2 (versioning) | otomatis | 30 hari (selaras retensi) |

## 7. Update aplikasi

```powershell
git pull; cd apps/api; fly deploy; cd ../web; fly deploy
```
Migrasi SQL jalan otomatis saat boot (`sqlx::migrate!`). Rollback: deploy image lama
`fly deploy -i registry.fly.io/transcribe-ai-api@sha256:…` (simpan sha rilis).
