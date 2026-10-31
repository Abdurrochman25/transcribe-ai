import { api, setAccessToken } from './client';
import type { AuthResponse, User } from './types';

export async function register(input: { name: string; email: string; password: string }) {
	const body = await api<AuthResponse>('/auth/register', {
		method: 'POST',
		body: input,
		auth: false
	});
	setAccessToken(body.tokens.access_token);
	return body.user;
}

export async function login(input: { email: string; password: string }) {
	const body = await api<AuthResponse>('/auth/login', { method: 'POST', body: input, auth: false });
	setAccessToken(body.tokens.access_token);
	return body.user;
}

export async function logout() {
	try {
		await api<void>('/auth/logout', { method: 'POST' });
	} finally {
		setAccessToken(null);
	}
}

export async function me() {
	const body = await api<{ user: User }>('/auth/me');
	return body.user;
}
