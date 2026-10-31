# 09 — Deployment & Infrastructure

## 1. Topologi Dev (docker-compose)

```
docker-compose up
├─ web        : SvelteKit (adapter-node)      → http://localhost:5173 (dev: vite)
├─ api        : Rust Axum                       → http://localhost:8080
├─ postgres   : PostgreSQL 16                   → localhost:5432
├─ minio      : S3-compatible storage + console → http://localhost:9001
└─ (sisi host) ffmpeg/ffprobe ada di dalam image api
```

`docker-compose.yml` (sketsa):

```yaml
services:
  postgres:
    image: postgres:16-alpine
    environment: { POSTGRES_USER: app, POSTGRES_PASSWORD: dev, POSTGRES_DB: transcribe }
    volumes: [pgdata:/var/lib/postgresql/data]
    healthcheck: { test: ["CMD-SHELL","pg_isready -U app"], interval: 5s, retries: 10 }

  minio:
    image: minio/minio
    command: server /data --console-address ":9001"
    environment: { MINIO_ROOT_USER: dev, MINIO_ROOT_PASSWORD: devdevdev }
    volumes: [miniodata:/data]
    ports: ["9000:9000","9001:9001"]

  api:
    build: { context: ., dockerfile: Dockerfile.api }
    environment:
      DATABASE_URL: postgres://app:dev@postgres:5432/transcribe
      S3_ENDPOINT: http://minio:9000
      S3_BUCKET: audio
      S3_ACCESS_KEY: dev
      S3_SECRET_KEY: devdevdev
      GROQ_API_KEY: ${GROQ_API_KEY}
      JWT_SECRET: ${JWT_SECRET}
      CORS_ALLOWED_ORIGINS: http://localhost:5173
    depends_on: { postgres: { condition: service_healthy }, minio: { condition: service_started } }
    ports: ["8080:8080"]

  web:
    build: { context: ., dockerfile: Dockerfile.web }
    environment:
      PUBLIC_API_URL: http://localhost:8080/api/v1
    depends_on: [api]
    ports: ["5173:3000"]

volumes: { pgdata: {}, miniodata: {} }
```

## 2. Container Images (multi-stage, kecil)

`Dockerfile.api` (sketsa):

```dockerfile
FROM rust:1.99-slim AS build
WORKDIR /app
COPY apps/api/ .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ffmpeg ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/transcribe-api /usr/local/bin/
ENV RUST_LOG=info
USER 1000
EXPOSE 8080
ENTRYPOINT ["transcribe-api"]
```

> Catatan build: gunakan `cargo-chef` untuk layer caching di CI (dari 8 menit → <2 menit).

`Dockerfile.web`: `node:22-slim` → `pnpm build` → jalankan `node build` (adapter-node), non-root, healthcheck `wget /healthz`.

## 3. Production (rekomendasi MVP: 1 VPS atau Fly.io)

```
Internet → Cloudflare (DNS, cache statis)
        → Caddy (auto-TLS) ─┬─ /api/*  → api:8080 (2 replica)
                            └─ /*      → web:3000
Postgres: Fly Postgres / Neon (managed, PITR)
Storage : Cloudflare R2 (S3-API, egress gratis — cocok untuk audio)
```

| Var lingkungan prod (contoh) | Nilai |
|---|---|
| `DATABASE_URL` | dari managed PG |
| `S3_ENDPOINT` / `S3_BUCKET` | R2 endpoint + bucket `transcribe-audio` |
| `JWT_SECRET`, `GROQ_API_KEY` | dari secret manager (fly secrets / systemd-creds) |
| `CORS_ALLOWED_ORIGINS` | `https://app.transcribeai.example` |
| `WORKER_CONCURRENCY` | 4–8 (sesuai rate-limit tier Groq) |
| `RUST_LOG` | `info,tower_http=warn` |

Skala berikutnya → lihat §8 dok 02 (pisahkan `--role worker`, antrean Redis).

## 4. CI/CD — GitHub Actions

`.github/workflows/ci.yml` (sketsa):

```yaml
name: CI
on: { push: { branches: [main] }, pull_request: {} }
jobs:
  api:
    runs-on: ubuntu-latest
    services:
      postgres: { image: postgres:16, env: {POSTGRES_USER: ci, POSTGRES_PASSWORD: ci, POSTGRES_DB: ci},
                  options: >- --health-cmd pg_isready --health-interval 5s --health-retries 10 }
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: "rustfmt, clippy" }
      - uses: Swatinem/rust-cache@v2
        with: { workspaces: "apps/api" }
      - run: cargo fmt --all -- --check
        working-directory: apps/api
      - run: cargo clippy --all-targets -- -D warnings
        working-directory: apps/api
      - run: cargo sqlx prepare --check          # pastikan query ter-compile-check
        working-directory: apps/api
      - run: cargo test
        working-directory: apps/api
        env: { DATABASE_URL: postgres://ci:ci@localhost/ci, GROQ_API_KEY: test, JWT_SECRET: ci-secret-ci-secret-ci-secret-1234 }
  web:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
      - run: pnpm install --frozen-lockfile
        working-directory: apps/web
      - run: pnpm check && pnpm lint && pnpm test
        working-directory: apps/web
      - run: pnpm build
        working-directory: apps/web
```

Deploy (CD): tag `v*` → build & push image ke GHCR → deploy Fly.io / watchtower di VPS. Release notes otomatis via Release-drafter.

## 5. Environment Matrix

| Env | Frontend | API | DB | Storage | Catatan |
|---|---|---|---|---|---|
| local | vite dev (5173) | cargo run (8080) | docker pg | MinIO | hot reload |
| ci | build saja | test + clippy | service pg | MinIO ephemeral? → testcontainers | Groq via wiremock |
| staging | image | image | managed | R2 folder `stg/` | data dummy, di-wipe mingguan |
| prod | image | 2+ replica | managed + PITR | R2 | backup + monitoring |

## 6. Monitoring & Ops Runbook

- **Health**: `/health` (liveness), `/ready` (DB+storage) — dipakai probe container/load balancer.
- **Log**: JSON stdout → dikumpulkan (Fly log / Loki); simpan 30 hari.
- **Metrik manual MVP**: satu dashboard (uptime + queue depth query SQL) — atau Grafana v1.1.
- **Alert**: ping eksternal (Better Stack) ke `/ready` tiap 1 mnt; alert jika queue > 50 job 15 menit.
- **Runbook** (ringkas):
  - Groq down (5xx massal) → job auto-backoff; jika >30 mnt, matikan worker (`WORKER_CONCURRENCY=0` deploy) dan nyalakan ulang setelah pulih; job reaper otomatis antre ulang.
  - DB penuh → purge `transcript_events`, aktifkan `purge_soft_deleted()`.
  - Migrasi berbahaya → backup snapshot dulu (`pg_dump`), migrasi di maintenance window, `migrate` forward-only dengan `--dry-run` review.

## 7. Backup

| Komponen | Frekuensi | Retensi |
|---|---|---|
| Postgres (managed PITR) | kontinu | 7 hari point-in-time |
| pg_dump mingguan ke R2 | mingguan | 4 snapshot |
| Audio (R2 versioning) | otomatis | 30 hari (selaras retensi) |
