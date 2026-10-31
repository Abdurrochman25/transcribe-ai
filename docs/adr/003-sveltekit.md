# ADR-003: Frontend — SvelteKit

**Status:** Accepted · **Tanggal:** 2026-10-31

## Konteks
Frontend bebas dipilih: butuh app interaktif (editor + player sinkron + SSE), SEO di landing page, bundle kecil, DX cepat.

## Alternatif
| Opsi | Plus | Minus |
|---|---|---|
| **SvelteKit 2 (Svelte 5)** ✅ | runes ($state) cocok untuk app stateful; bundle kecil; SSR + SPA island; built-in routing/load; sedikit boilerplate | talent pool lebih kecil; beberapa lib UI lebih kaya di React |
| Next.js (React) | ekosistem terbesar, hiring mudah | boilerplate lebih besar; RSC complexity untuk app internal |
| Vue/Nuxt | tengah | — |
| HTMX + SSR Rust | tanpa JS build | editor interaktif kompleks jadi sulit |

## Keputusan
SvelteKit 2 + Svelte 5 runes + TypeScript + Tailwind 4 + adapter-node (deploy sama seperti API: Docker).

## Konsekuensi
(+) Perf bagus untuk daftar segment ribuan item (runes reactivity granular + virtualisasi).
(−) Ekosistem komponen lebih kecil → bangun komponen UI sendiri (ui/) sesuai token dok 07.
