/**
 * Security headers untuk semua respons web — docs/08 §3.
 * CSP hanya di produksi (dev butuh HMR/inject Vite).
 * HSTS ditangani proxy/TLS di depan (Caddy/Cloudflare — docs/11 runbook).
 *
 * Catatan template: `import type { Handle } from '@sveltejs/kit'` tidak bisa
 * dipakai — kit 3.1.1 tidak mengekspor `./types` sehingga deklarasi ambient
 * gagal dimuat svelte-check (pola workaround yang sama dengan $lib). Tipe
 * struktural lokal di bawah ekuivalen untuk hook yang hanya mem-pass event.
 */
const apiOrigin = (() => {
	const url = import.meta.env.PUBLIC_API_URL ?? 'http://localhost:8080/api/v1';
	try {
		return new URL(url).origin;
	} catch {
		return 'http://localhost:8080';
	}
})();

type HandleHook = (input: {
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	event: any;
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	resolve: (event: any, opts?: any) => Promise<Response>;
}) => Promise<Response>;

export const handle: HandleHook = async ({ event, resolve }) => {
	const response = await resolve(event);

	response.headers.set('X-Content-Type-Options', 'nosniff');
	response.headers.set('X-Frame-Options', 'DENY');
	response.headers.set('Referrer-Policy', 'strict-origin-when-cross-origin');

	if (!import.meta.env.DEV) {
		response.headers.set(
			'Content-Security-Policy',
			[
				"default-src 'self'",
				"script-src 'self'",
				// Svelte scoped style + Tailwind: stylesheet eksternal + inline kecil.
				"style-src 'self' 'unsafe-inline'",
				"img-src 'self' data:",
				// fetch/XHR/SSE ke API.
				`connect-src 'self' ${apiOrigin}`,
				// elemen <audio> streaming dari API.
				`media-src 'self' ${apiOrigin} blob:`,
				"font-src 'self' data:",
				"object-src 'none'",
				"base-uri 'self'",
				"form-action 'self'",
				"frame-ancestors 'none'"
			].join('; ')
		);
	}

	return response;
};
