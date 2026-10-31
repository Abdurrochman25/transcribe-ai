# ADR-004: Data access — PostgreSQL + SQLx

**Status:** Accepted · **Tanggal:** 2026-10-31

## Konteks
Butuh DB relasional (transcript+segments relasional, transaksi job) dan data layer yang ter-check saat compile.

## Alternatif
| Opsi | Plus | Minus |
|---|---|---|
| **SQLx + Postgres** ✅ | SQL asli + compile-time checked (query! / macros), migrasi bawaan, async, tanpa ORM magic; fitur PG (ENUM, SKIP LOCKED, partial index) langsung dipakai | query ditulis SQL manual |
| SeaORM | ergonomic, relational mapper | abstraksi tambahan, debug query lebih jauh |
| Diesel | matang, cepat | sync-first, DSL curam |
| Prisma-like (tokio) | — | tidak matang di Rust |

**SQLite?** Ditolak untuk default dev: fitur yang diandalkan (ENUM, `FOR UPDATE SKIP LOCKED`, citext, partial index) spesifik Postgres — dev harus identik dengan prod (docker pg sudah mudah).

## Keputusan
PostgreSQL 16 + SQLx. Query pooling `PgPool` (default). Offline mode (`sqlx prepare`) untuk CI tanpa DB… CI tetap pakai service postgres.

## Konsekuensi
(+) Ketidakcocokan SQL/types ketahuan di compile time → bug turun drastis.
(−) Developer harus nyaman menulis SQL.
