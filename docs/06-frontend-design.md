# 06 — Frontend Design (SvelteKit)

## 1. Stack

| Layer | Pilihan | Catatan |
|---|---|---|
| Framework | SvelteKit 2 + Svelte 5 (runes) | SSR untuk landing/auth, CSR untuk app |
| Bahasa | TypeScript strict | |
| Styling | Tailwind CSS 4 | + CSS variables untuk design tokens (07) |
| Adapter | `adapter-node` | SSR + API proxy-able; deploy fleksibel (Docker/VPS/Fly) |
| API Client | `openapi-typescript`-generated types (ops.) / handmade typed client `lib/api` | kontrak mengikuti docs 03 |
| Data fetching | SvelteKit `load` + custom `lib/stores/job.svelte.ts` (SSE) | tanpa react-query; runes cukup |
| Form | SvelteKit form actions + progressive enhancement | |
| Icon | lucide-svelte | |
| Audio player | HTML5 `<audio>` custom controls | |

## 2. Struktur Project

```
apps/web/
├── src/
│   ├── routes/
│   │   ├── +layout.svelte            # shell: sidebar/topbar, theme
│   │   ├── +layout.server.ts         # session guard (cookie access token)
│   │   ├── (public)/
│   │   │   ├── +page.svelte          # landing
│   │   │   ├── login/+page.svelte
│   │   │   └── register/+page.svelte
│   │   └── (app)/
│   │       ├── +layout.ts            # ensureloggedin, redirect /login
│   │       ├── dashboard/+page.svelte        # riwayat + upload cepat
│   │       ├── upload/+page.svelte           # wizard upload opsi lengkap
│   │       ├── t/[id]/+page.svelte           # detail: player + editor + export
│   │       └── settings/+page.svelte         # profil, preferensi bahasa/theme
│   ├── lib/
│   │   ├── api/
│   │   │   ├── client.ts             # fetch wrapper: auth header, refresh-on-401, retry
│   │   │   ├── auth.ts, transcripts.ts, types.ts
│   │   │   └── sse.ts                # subscribeProgress(id, onEvent) + auto-reconnect
│   │   ├── components/
│   │   │   ├── upload/FileDropzone.svelte
│   │   │   ├── upload/UploadProgress.svelte
│   │   │   ├── transcript/Player.svelte       # waveform-lite + controls
│   │   │   ├── transcript/SegmentList.svelte  # virtualized list
│   │   │   ├── transcript/SegmentItem.svelte  # inline edit
│   │   │   ├── transcript/ExportMenu.svelte
│   │   │   └── ui/                           # Button, Badge, Dialog, Toast, ...
│   │   ├── stores/
│   │   │   ├── auth.svelte.ts        # $state user, login/logout
│   │   │   ├── job.svelte.ts         # $state progress map + SSE wiring
│   │   │   └── theme.svelte.ts
│   │   └── utils/time.ts, format.ts, srt.ts
│   ├── app.css                       # tailwind + tokens
│   └── app.d.ts
├── package.json
├── svelte.config.js                  # adapter-node + routes
├── vite.config.ts
└── tsconfig.json
```

## 3. Kontrak API Client

```ts
// lib/api/client.ts (perilaku inti)
- baseURL dari env PUBLIC_API_URL (default '/api/v1', di-proxy adapter)
- access token: memory module-level (bukan localStorage → mitigasi XSS)
- refresh: otomatis saat 401 (single-flight lock, retry request sekali)
- SSE: EventSource tidak bawa auth header → pakai short-lived query token
  POST /auth/refresh → set cookie; SSE cookie terkirim; fallback polling 3s
- error dinormalisasi ke ApiError { code, message, details? }
```

```ts
// lib/api/types.ts (potongan)
export type TranscriptStatus = 'uploaded'|'queued'|'validating'|'chunking'
  |'transcribing'|'processing'|'completed'|'failed'|'cancelled';
export interface Segment { id:number; idx:number; start_ms:number; end_ms:number;
  text:string; speaker:string|null; edited:boolean }
export interface Transcript { id:string; title:string; status:TranscriptStatus;
  progress:number; language:string|null; duration_seconds:number|null; ... }
```

## 4. State Management (runes)

```ts
// lib/stores/job.svelte.ts (sketsa)
class JobStore {
  progress = $state<Record<string, JobState>>({});   // id → live status

  watch(id: string) {
    if (this.progress[id]) return;
    this.progress[id] = { status:'queued', progress:0 };
    subscribeProgress(id, (ev) => {
      this.progress[id] = ev;                        // progress | completed | failed
      if (ev.status === 'completed') invalidateAll(); // refresh data transcript
    });
  }
}
export const jobs = new JobStore();
```

- Server data (`load`) = sumber kebenaran; `job.svelte.ts` hanya overlay progres realtime.
- `invalidateAll()` SvelteKit dipanggil saat event `completed`.

## 5. Routing & Guards

| Route | Akses | `load` behavior |
|---|---|---|
| `/`, `/login`, `/register` | publik | redirect ke `/dashboard` bila sudah login |
| `(app)/*` | login | `(app)/+layout.ts` → `redirect(302,'/login?next=...')` bila tanpa session |
| `/t/[id]` | pemilik | 404 page bila API balas 404 (jangan bocorkan eksistensi id) |

Session check SSR: cookie `rt` ada → server-side cek/refresh access token → set user di `locals`.

## 6. Halaman & Komponen Utama

### Dashboard
- `+page.server.ts load`: `GET /transcripts` (server-side, bawa token dari cookie session).
- Item list: badge status (warna per state), progress bar live untuk job aktif (`jobs.watch`).
- Search input (debounce 300ms) + filter status + pagination.

### Upload (`/upload`)
1. **Dropzone** — validasi client (ekstensi + size ≤ MAX), preview nama/ukuran/durasi (Web Audio API decode cepat bila memungkinkan).
2. **Opsi** — judul, bahasa (select `GET /languages` + "Auto detect"), model (turbo vs large), prompt hint (textarea, hint: "tulis nama orang/istilah agar akurat").
3. **Submit** — `POST /transcripts` (XMLHttpRequest untuk progress upload byte) → tampilkan `UploadProgress` (byte → lalu job progress SSE) → selesai → navigate `/t/[id]`.

### Transcript detail `/t/[id]`
- Kiri: `Player` (audio via presigned URL) + metadata (bahasa terdeteksi, durasi, model, waktu).
- Kanan: `SegmentList` — tiap `SegmentItem`: timestamp klik (`player.currentTime = start_ms/1000`), teks; klik teks → `contenteditable` → save (PATCH, debounce 800ms / tombol Save), indikator `edited`.
- Header: judul (editable inline), `ExportMenu`, tombol Retry/Cancel sesuai status.
- Auto-scroll mengikuti playback (`currentTime` → highlight segment aktif).
- State gagal: pesan error manusiawi + tombol **Coba lagi**.

### Settings
- Profil (nama, avatar URL), preferensi bahasa default, theme, tombol logout semua sesi (revoke semua refresh token — endpoint v1.1).

## 7. Performance & A11y

- `SegmentList` virtualisasi (ribuan segment) — render window ±30 item.
- Presigned audio: `preload="none"` sampai user play.
- Font: `font-variant-numeric: tabular-nums` untuk timestamp.
- A11y: dropzone keyboard-accessible, `aria-live` untuk status job, fokus ring jelas, kontras AA.
- Bundle: code-split per route (bawaan SvelteKit); target LCP < 2.5s di 3G cepat.

## 8. Testing

- Unit: `vitest` untuk utils (`srt.ts`, format waktu).
- Komponen: `@testing-library/svelte` untuk SegmentItem & Dropzone (mock `lib/api`).
- E2E: `playwright` happy-path (login → upload mp3 fixture 1 detik → tunggu completed → export srt) — jalan di CI dengan backend compose + Groq mock.
