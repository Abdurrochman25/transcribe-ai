# ADR-005: Gaya arsitektur — Modular Monolith + antrean berbasis DB

**Status:** Accepted · **Tanggal:** 2026-10-31

## Konteks
Tim kecil, rilis cepat. Microservices = overhead ops (deploy, tracing, konsistensi) yang belum dibutuhkan; tapi arsitektur harus bisa diekstrak nanti.

## Keputusan
1. **Satu binary backend** (`apps/api`) dengan batas modul keras: `routes → services → repositories/domain`, `services → infra`. Domain murni tanpa I/O → mudah di-test.
2. **Antrean job di PostgreSQL** (`status='queued'` + `FOR UPDATE SKIP LOCKED`), bukan Redis/NATS: atomik bersama status job dalam satu transaksi; polling 2 dtk + upgrade path `LISTEN/NOTIFY` (v1.1) → antrean eksternal (v3 bila perlu).
3. **Peran proses via flag** `--role api|worker|all` — kode sama, skalabilitas beda: MVP jalan `all`; prod besar pisah worker.

## Konsekuensi
(+) Satu deployable, transaksi lintas concern, observabilitas sederhana.
(+) Path ekstraksi jelas: batas modul = calon service boundary (transcription worker → service sendiri).
(−) Polling menambah latensi ±2 dtk (diterima di MVP; dihapus LISTEN/NOTIFY).
(−) Load transkripsi memengaruhi kapasitas API bila tidak dipisah — mitigasi flag role & scaling path dok 02 §8.
