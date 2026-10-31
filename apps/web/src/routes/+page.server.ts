/**
 * Landing page — Fase 0: cek kesehatan API untuk membuktikan wiring full-stack.
 * (Laman publik lengkap datang di Fase 5 — lihat docs/10-roadmap.md)
 *
 * Catatan: file ini hanya berjalan di server (adapter-node), jadi `process.env`
 * aman dipakai dan tetap terbaca saat runtime (bukan build-time).
 * Kontrak `PUBLIC_API_URL` mengikuti docs/06-frontend-design.md §3.
 */
export const load = async ({ fetch }) => {
	// PUBLIC_API_URL berformat ".../api/v1"; /health ada di root API.
	const apiUrl = process.env.PUBLIC_API_URL || 'http://localhost:8080/api/v1';
	const apiServer = apiUrl.replace(/\/api\/v1\/?$/, '');

	let api: { online: boolean; version: string | null } = { online: false, version: null };
	try {
		const res = await fetch(`${apiServer}/health`, {
			signal: AbortSignal.timeout(3000)
		});
		if (res.ok) {
			const body = (await res.json()) as { status?: string; version?: string };
			api = { online: body.status === 'ok', version: body.version ?? null };
		}
	} catch {
		// API belum berjalan — tampilkan status di UI, jangan gagaskan halaman.
	}

	return { api, apiUrl };
};
