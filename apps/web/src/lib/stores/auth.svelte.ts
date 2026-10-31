/**
 * Store auth global (Svelte 5 runes).
 * Access token hidup di memori (lib/api/client); store hanya menyimpan user + status.
 */

import { goto } from '$app/navigation';
import * as authApi from '../api/auth';
import type { User } from '../api/types';

class AuthStore {
	user = $state<User | null>(null);
	/** true setelah pemeriksaan sesi awal selesai (guard menunggu ini). */
	ready = $state(false);

	#initialized = false;

	/** Dipanggil layout (app): cek sesi via /auth/me (+ refresh bila access expired). */
	async init() {
		if (this.#initialized) return;
		this.#initialized = true;
		try {
			this.user = await authApi.me();
		} catch {
			this.user = null;
		} finally {
			this.ready = true;
		}
	}

	async login(email: string, password: string) {
		this.user = await authApi.login({ email, password });
	}

	async register(name: string, email: string, password: string) {
		this.user = await authApi.register({ name, email, password });
	}

	async logout() {
		await authApi.logout();
		this.user = null;
		this.#initialized = false;
		this.ready = false;
		await goto('/login');
	}
}

export const auth = new AuthStore();
