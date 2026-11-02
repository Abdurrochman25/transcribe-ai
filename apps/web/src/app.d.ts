// See https://svelte.dev/docs/kit/types#app.d.ts
// for information about these interfaces
declare global {
	namespace App {
		// interface Error {}
		// interface Locals {}
		// interface PageData {}
		// interface PageState {}
		// interface Platform {}
	}

	interface ImportMetaEnv {
		/** Base URL API, format ".../api/v1" — docs/06-frontend-design.md §3 */
		readonly PUBLIC_API_URL?: string;
		/** Ditambahkan Vite — dipakai hooks.server.ts (CSP prod-only). */
		readonly DEV?: boolean;
		readonly MODE?: string;
	}
}

export {};
