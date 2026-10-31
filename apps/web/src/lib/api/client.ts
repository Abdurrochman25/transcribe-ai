/**
 * Fetch wrapper API — docs/06-frontend-design.md §3.
 * - Access token hanya di memori modul (mitigasi XSS, bukan localStorage).
 * - 401 → refresh single-flight (cookie httpOnly dikirim otomatis, credentials include)
 *   lalu request diulang sekali.
 * - Error dinormalisasi ke ApiError.
 */

import { ApiError } from './types';

const BASE: string = import.meta.env.PUBLIC_API_URL ?? 'http://localhost:8080/api/v1';

export const API_BASE = BASE;

let accessToken: string | null = null;

export function setAccessToken(token: string | null) {
	accessToken = token;
}

export function getAccessToken() {
	return accessToken;
}

interface ApiOptions {
	method?: 'GET' | 'POST' | 'PATCH' | 'DELETE';
	body?: unknown;
	/** false untuk endpoint publik (login/register/refresh). */
	auth?: boolean;
}

let refreshInFlight: Promise<boolean> | null = null;

async function refreshTokens(): Promise<boolean> {
	// Single-flight: banyak request 401 bersamaan → hanya satu panggilan refresh.
	if (!refreshInFlight) {
		refreshInFlight = (async () => {
			try {
				const res = await fetch(`${BASE}/auth/refresh`, {
					method: 'POST',
					credentials: 'include'
				});
				if (!res.ok) return false;
				const body = (await res.json()) as { tokens?: { access_token: string } };
				if (!body.tokens?.access_token) return false;
				accessToken = body.tokens.access_token;
				return true;
			} catch {
				return false;
			} finally {
				refreshInFlight = null;
			}
		})();
	}
	return refreshInFlight;
}

async function parseError(res: Response): Promise<never> {
	let code = 'INTERNAL';
	let message = 'Terjadi kesalahan. Coba lagi nanti.';
	let details: undefined | { field: string; issue: string }[];
	let request_id: string | undefined;
	try {
		const body = (await res.json()) as {
			error?: { code?: string; message?: string; details?: typeof details; request_id?: string };
		};
		if (body.error) {
			code = body.error.code ?? code;
			message = body.error.message ?? message;
			details = body.error.details;
			request_id = body.error.request_id;
		}
	} catch {
		// respons non-JSON — pakai default
	}
	throw new ApiError(res.status, code, message, details, request_id);
}

export async function api<T>(path: string, options: ApiOptions = {}): Promise<T> {
	const { method = 'GET', body, auth = true } = options;

	const doFetch = () =>
		fetch(`${BASE}${path}`, {
			method,
			credentials: 'include',
			headers: {
				...(body !== undefined ? { 'Content-Type': 'application/json' } : {}),
				...(auth && accessToken ? { Authorization: `Bearer ${accessToken}` } : {})
			},
			body: body !== undefined ? JSON.stringify(body) : undefined
		});

	let res = await doFetch();

	// 401 pada endpoint terproteksi → coba refresh sekali, ulangi request.
	if (res.status === 401 && auth && accessToken) {
		const refreshed = await refreshTokens();
		if (refreshed) {
			res = await doFetch();
		}
	}

	if (!res.ok) {
		await parseError(res);
	}

	if (res.status === 204) return undefined as T;
	return (await res.json()) as T;
}
