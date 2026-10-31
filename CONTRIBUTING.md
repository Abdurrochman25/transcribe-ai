# Kontribusi ke TranscribeAI

Terima kasih ingin berkontribusi! 🙌

## Alur Kerja

1. **Issue dulu** — untuk bug/fitur baru, buka issue agar bisa didiskusikan sebelum koding.
2. **Fork & branch** — nama branch: `feat/<topik>`, `fix/<topik>`, `docs/<topik>`.
3. **PR kecil** — satu PR satu fokus; sertakan konteks *mengapa*, bukan hanya *apa*.
4. **Review** — minimal self-review; maintainer akan review sebelum merge (squash).

## Commit Message

Konvensi [Conventional Commits](https://www.conventionalcommits.org/):
```
feat(api): tambah endpoint retry job transkripsi
fix(web): perbaiki auto-scroll segment saat playback
docs: tambah ADR antrean berbasis DB
```

## Checklist PR

**Backend (`apps/api`)**
- [ ] `cargo fmt --all` bersih
- [ ] `cargo clippy --all-targets -- -D warnings` bersih
- [ ] `cargo test` hijau (termasuk test baru untuk perilaku yang diubah)
- [ ] Query baru lewat `sqlx::query!` / sudah di-prepare (`cargo sqlx prepare`)

**Frontend (`apps/web`)**
- [ ] `pnpm check` dan `pnpm lint` bersih
- [ ] `pnpm test` hijau; komponen baru punya test bila logika non-trivial
- [ ] Aksesibel: keyboard + label + kontras (lihat docs/07-ui-design.md)

**Umum**
- [ ] Tidak ada secret/kredensial dalam diff
- [ ] Docs diperbarui bila perilaku/API berubah (docs/03-api-spec.md dll.)

## Menjalankan Dev

```bash
docker compose up -d postgres minio
cp .env.example .env            # isi GROQ_API_KEY & JWT_SECRET
# api
cd apps/api && cargo run
# web (terminal lain)
cd apps/web && pnpm install && pnpm dev
```

## Kode Etik

Jadilah profesional dan berempati. Kritik ide, bukan orang.
