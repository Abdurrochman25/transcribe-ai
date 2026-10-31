# 02 — System Architecture

## 1. Diagram High-Level

```mermaid
graph TB
    subgraph Client["🖥️ Browser"]
        UI["SvelteKit App<br/>(SSR + SPA islands)"]
    end

    subgraph Edge["🌐 Edge / Reverse Proxy (Caddy/Nginx/Cloudflare)"]
        TLS["TLS termination"]
        STATIC["Static assets (SSR build)"]
    end

    subgraph API["🦀 Backend Rust (Axum) — apps/api"]
        ROUTES["Routes / Handlers<br/>auth · transcripts · export"]
        MW["Middleware<br/>JWT auth · rate limit · CORS · tracing"]
        SVC["Services<br/>business logic"]
        REPO["Repositories (SQLx)"]
        WORKER["Transcription Worker<br/>(tokio task pool + queue in-DB)"]
        SSE["SSE Hub<br/>(broadcast per transcript)"]
        GROQ["Groq Client<br/>(reqwest multipart)"]
        FFMPEG["ffmpeg/ffprobe<br/>(subprocess)"]
    end

    subgraph Data["💾 Data Layer"]
        PG[("PostgreSQL 16")]
        S3[("Object Storage<br/>MinIO / S3 / R2")]
    end

    EXT["☁️ Groq API<br/>api.groq.com"]

    UI -->|HTTPS /api/v1| TLS --> MW --> ROUTES
    UI <-.->|"SSE: /transcripts/:id/events"| SSE
    ROUTES --> SVC --> REPO --> PG
    SVC --> S3
    WORKER --> REPO
    WORKER --> GROQ --> EXT
    WORKER --> FFMPEG
    WORKER --> SSE
    WORKER --> S3
```

**Prinsip:** *modular monolith* — satu deployable backend, batas modul jelas di level crate/module, sehingga bisa diekstrak jadi service terpisah kalau nanti perlu (lihat [ADR-005](adr/005-modular-monolith.md)).

## 2. Komponen & Tanggung Jawab

### 2.1 Edge / Reverse Proxy
- TLS, gzip/brotli, serve static build SvelteKit (atau di belakang adapter-node).
- Path routing: `/api/*` → backend Rust; sisanya → frontend.
- Upload body limit (mis. 200 MB) sebelum masuk backend.

### 2.2 Backend Rust (`apps/api`)
| Modul | Tanggung jawab |
|---|---|
| `routes/` | HTTP handler, parsing/validasi request, mapping ke response. Tanpa business logic. |
| `services/` | Orkestrasi: `TranscriptService`, `AuthService`, `ExportService`. Unit of work per operasi. |
| `repositories/` | Query SQL via SQLx (compile-time checked). Satu repo per aggregate. |
| `domain/` | Entity + enum status + **state machine** job + invarian. Murni, tanpa I/O. |
| `workers/` | Consumer antrean transkripsi: validasi → (chunk) → panggil Groq → simpan segments → publish SSE. |
| `infra/` | Klien eksternal: `GroqClient`, `ObjectStore`, `SseHub`, ffmpeg wrapper. |
| `config.rs` | Semua dari env (`FIGment`/`serde`), fail-fast saat start. |

### 2.3 Database (PostgreSQL)
- Source of truth untuk user, transcript, segments, token, audit.
- **Antrean job berbasis DB**: row `transcripts` dengan `status='queued'` diambil worker dengan `SELECT ... FOR UPDATE SKIP LOCKED` → tidak butuh Redis/RabbitMQ di fase awal (lihat §5).

### 2.4 Object Storage
- Menyimpan file audio asli (private bucket) — DB hanya menyimpan `storage_key`.
- Lifecycle: hapus otomatis setelah `RETENTION_DAYS` (MinIO ILM / S3 lifecycle rule).
- Backend membuat **pre-signed URL** untuk streaming audio ke player frontend (jangka pendek, mis. 15 menit).

### 2.5 Frontend (SvelteKit)
- SSR untuk halaman publik (landing/login), client-side untuk app (dashboard, editor).
- Konsumsi REST + SSE; tanpa state server global (state = server backend).

## 3. Alur Data End-to-End

### 3.1 Sequence: Upload → Transkripsi → Selesai

```mermaid
sequenceDiagram
    actor U as User
    participant W as SvelteKit
    participant A as Axum API
    participant S3 as Object Storage
    participant DB as PostgreSQL
    participant K as Worker (tokio)
    participant G as Groq API

    U->>W: Drag & drop audio
    W->>A: POST /api/v1/transcripts (multipart)
    A->>A: validasi tipe/ukuran (magic bytes)
    A->>S3: PutObject (streaming)
    A->>DB: INSERT transcripts (status=uploaded)
    A-->>W: 202 { id, status: "queued" }
    Note over K: worker poll antrean (FOR UPDATE SKIP LOCKED)
    K->>DB: claim job → status=processing
    K->>S3: GetObject (stream)
    K->>K: ffprobe → durasi; >25MB? → chunk via ffmpeg
    loop per chunk
        K->>G: POST /audio/transcriptions (multipart)
        G-->>K: verbose_json (segments)
        K->>DB: UPDATE progress; INSERT segments (offset digeser)
        K-->>W: SSE event: progress
    end
    K->>DB: status=completed, gabung segment
    K-->>W: SSE event: completed
    W->>A: GET /transcripts/:id → render editor
```

### 3.2 Sequence: Auth (login + refresh)

```mermaid
sequenceDiagram
    actor U as User
    participant W as SvelteKit
    participant A as Axum API
    participant DB as PostgreSQL

    U->>W: submit login
    W->>A: POST /api/v1/auth/login
    A->>DB: SELECT user by email
    A->>A: Argon2 verify
    A->>DB: INSERT refresh_token (hashed)
    A-->>W: 200 { access_token(15m), refresh_token(30d) }
    Note over W: access token di memory + refresh di httpOnly cookie
    W->>A: (tiap request) Authorization: Bearer
    alt token expired
        W->>A: POST /auth/refresh (cookie refresh)
        A->>DB: rotate: revoke lama, insert baru
        A-->>W: access_token baru
    end
```

## 4. State Machine Job Transkripsi

```mermaid
stateDiagram-v2
    [*] --> uploaded: POST /transcripts
    uploaded --> queued: file tersimpan di S3
    queued --> validating: worker claim
    validating --> failed: ffprobe error / format invalid
    validating --> chunking: size > limit
    validating --> transcribing: size ≤ limit
    chunking --> transcribing
    transcribing --> retrying: error 429/5xx (backoff)
    retrying --> transcribing: max 3x
    transcribing --> processing: gabung & offset timestamp
    processing --> completed
    transcribing --> failed: error non-retryable / retry habis
    queued --> cancelled: user cancel
    transcribing --> cancelled: user cancel
    failed --> queued: POST /retry
    completed --> [*]
```

Invarian (dijaga di `domain/`):
- Transisi hanya via jalur yang diizinkan; transisi lain → error `InvalidStateTransition`.
- `progress` monoton naik; `completed_at` hanya terisi di `completed`.
- `cancelled` hanya bisa dari status aktif (`queued`/`validating`/`chunking`/`transcribing`).

## 5. Desain Antrean Job (DB-based)

**Kenapa tidak Redis/SQS dulu?** Satu dependency lebih sedikit, atomisitas job+status dalam transaksi DB, cukup untuk throughput fase awal (target: ratusan job/jam). Trade-off: polling interval menambah latensi ±2–5 detik — mitigasi: `LISTEN/NOTIFY` PostgreSQL untuk wake worker instan (roadmap v1.1).

```sql
-- Worker claim (satu query atomik)
UPDATE transcripts SET status='validating', updated_at=now()
WHERE id = (
  SELECT id FROM transcripts
  WHERE status='queued'
  ORDER BY created_at
  FOR UPDATE SKIP LOCKED LIMIT 1
)
RETURNING id, storage_key;
```

Konkurensi worker: `N` tokio task (default 4), configurable `WORKER_CONCURRENCY`. Cancellation: worker cek `cancel_requested` flag tiap antara chunk.

## 6. Realtime Progress (SSE)

- `GET /api/v1/transcripts/:id/events` → `text/event-stream`.
- `SseHub`: `DashMap<TranscriptId, Vec<mpsc::Sender>>`; worker publish event → hub fan-out.
- Event: `{"type":"progress","data":{"status":"transcribing","progress":42,"message":"chunk 2/5"}}`.
- Heartbeat tiap 15 detik (comment frame) agar proxy tidak memutus.
- Fallback: polling `GET /transcripts/:id` tiap 3 detik bila SSE diblokir proxy.

## 7. Strategi Keandalan

| Kegagalan | Penanganan |
|---|---|
| Groq 429 / 5xx | Retry exponential backoff + jitter (max 3), hormati `Retry-After` |
| Worker crash saat job | Job stuck >stale-timeout (10 mnt) diambil ulang (status di-reset oleh reaper task) |
| Upload putus | Client chunked upload / presigned multipart (v2; MVP: batas ukuran + retry upload dari awal) |
| DB down | Health check gagal → readiness probe → proxy stop routing; request 503 |
| File korup | ffprobe gagal → status `failed` dengan pesan jelas |

## 8. Scaling Path

```mermaid
graph LR
    M1["MVP<br/>1 VPS: api+web+pg+minio<br/>(compose)"] --> M2["Grow<br/>api 2-3 replica (stateless)<br/>pg managed (RDS/Neon)<br/>S3+CF"] --> M3["Scale<br/>worker terpisah & autoscale<br/>antrean Redis/NATS<br/>read replica"]
```

- Backend **stateless** (JWT + DB + S3) → horizontal scale hanya dengan menambah replica.
- Pemisahan `api` vs `worker` di binary yang sama via flag `--role api|worker|all` — di prod besar, jalankan keduanya terpisah.

## 9. Keputusan Penting (ringkas → ADR lengkap)

| Keputusan | Pilihan | Alasan |
|---|---|---|
| Web framework | Axum | Ekosistem tokio, middleware tower, typing kuat, komunitas besar |
| ORM | SQLx | Compile-time checked SQL tanpa ORM magic |
| Job queue | DB-based | Simpel & transaksional; upgrade path jelas |
| Realtime | SSE | One-way, murah, auto-reconnect bawaan browser; WS hanya perlu di v2 (streaming) |
| Audio processing | ffmpeg subprocess | De-facto standard; hindari rewrite Rust binding |
