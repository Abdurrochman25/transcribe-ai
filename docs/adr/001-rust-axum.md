# ADR-001: Web framework Rust — Axum

**Status:** Accepted · **Tanggal:** 2026-10-31

## Konteks
Butuh HTTP framework async untuk API transkripsi: banyak I/O (upload stream, panggilan Groq, SSE), typing kuat, ekosistem matang.

## Alternatif
| Opsi | Plus | Minus |
|---|---|---|
| **Axum** ✅ | Di atas tower/tokio, middleware komposabel, extractor typing, komunitas terbesar saat ini,维护 aktif oleh tim tokio-rs | API kadang berubah antar versi major (0.6→0.7→0.8) |
| Actix-web | Performa tinggi, matang | Ekosistem middleware lebih "framework-specific", actor model legacy membingungkan |
| Rocket | DX enak, docs bagus | Community lebih kecil, magic macro lebih banyak |
| Poem / Salvo | ringan | ekosistem lebih kecil |

## Keputusan
**Axum 0.8** + tower-http. SSE via `axum::response::sse`, multipart bawaan.

## Konsekuensi
(+) Test in-proc mudah (`tower::ServiceExt::oneshot`), komposisi layer rapi.
(−) Perlu disiplin upgrade major version (cek changelog middleware pihak ketiga).
