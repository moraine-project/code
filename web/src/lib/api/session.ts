import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';
import { z } from 'zod';

export const accountSchema = z.object({
	user_id: z.string(),
	email: z.string(),
	via: z.string(),
});

export type Account = z.infer<typeof accountSchema>;

let csrfInMemory: string | null = null;

export function setCsrfToken(token: string | null) {
	csrfInMemory = token;
}

export function apiBase(): string {
	const configured = (PUBLIC_MORAINE_REGISTRY ?? '').replace(/\/+$/, '');
	if (!configured) {
		return '';
	}
	if (typeof window !== 'undefined' && configured === window.location.origin) {
		return '';
	}
	return configured;
}

export function apiOrigin(): string {
	return apiBase() || (typeof window !== 'undefined' ? window.location.origin : '');
}

export function csrfToken(): string | null {
	if (csrfInMemory) {
		return csrfInMemory;
	}
	if (typeof document === 'undefined') {
		return null;
	}
	const entry = document.cookie.split('; ').find((row) => row.startsWith('moraine_csrf='));
	return entry ? decodeURIComponent(entry.slice('moraine_csrf='.length)) : null;
}

export async function authorizedFetch(path: string, init: RequestInit = {}): Promise<Response> {
	const base = apiBase();
	const headers = new Headers(init.headers);
	const token = csrfToken();
	if (token) {
		headers.set('x-csrf-token', token);
	}
	return fetch(`${base}${path}`, {
		credentials: base ? 'include' : 'same-origin',
		...init,
		headers,
	});
}

export async function account(): Promise<Account | null> {
	const response = await authorizedFetch('/v1/auth/me');
	if (response.status === 401) {
		return null;
	}
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	return accountSchema.parse(await response.json());
}

export async function login(email: string, password: string): Promise<void> {
	const response = await authorizedFetch('/v1/auth/session', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ email, password }),
	});
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	const body = await response.json().catch(() => null);
	if (body && typeof body.csrf === 'string') {
		setCsrfToken(body.csrf);
	}
}

export async function register(email: string, password: string): Promise<void> {
	const response = await authorizedFetch('/v1/auth/register', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ email, password }),
	});
	if (!response.ok) {
		throw new Error(await failure(response));
	}
}

export async function logout(): Promise<void> {
	const response = await authorizedFetch('/v1/auth/session', { method: 'DELETE' });
	if (!response.ok && response.status !== 401) {
		throw new Error(await failure(response));
	}
	setCsrfToken(null);
}

export async function failure(response: Response): Promise<string> {
	const text = await response.text();
	return text || `request failed (${response.status})`;
}
