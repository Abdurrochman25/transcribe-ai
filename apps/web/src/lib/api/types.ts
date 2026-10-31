/** Kontrak API — docs/03-api-spec.md & docs/06-frontend-design.md */

export interface User {
	id: string;
	email: string;
	name: string;
	plan: string;
	created_at: string;
}

export interface Tokens {
	access_token: string;
	refresh_token?: string;
	expires_in: number;
}

export interface FieldError {
	field: string;
	issue: string;
}

export interface AuthResponse {
	user: User;
	tokens: Tokens;
}

/** Error ter-normalisasi dari envelope `{ error: {...} }` API. */
export class ApiError extends Error {
	constructor(
		public status: number,
		public code: string,
		message: string,
		public details?: FieldError[],
		public request_id?: string
	) {
		super(message);
		this.name = 'ApiError';
	}
}
