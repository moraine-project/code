import { z } from 'zod';

export const accountSchema = z.object({
	user_id: z.string(),
	email: z.string(),
	via: z.string()
});

export type Account = z.infer<typeof accountSchema>;

export function csrfToken(): string | null {
	const entry = document.cookie.split('; ').find((row) => row.startsWith('moraine_csrf='));
	return entry ? decodeURIComponent(entry.slice('moraine_csrf='.length)) : null;
}

export async function authorizedFetch(path: string, init: RequestInit = {}): Promise<Response> {
	const headers = new Headers(init.headers);
	const token = csrfToken();
	if (token) {
		headers.set('x-csrf-token', token);
	}
	return fetch(path, { credentials: 'same-origin', ...init, headers });
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
		body: JSON.stringify({ email, password })
	});
	if (!response.ok) {
		throw new Error(await failure(response));
	}
}

export async function register(email: string, password: string): Promise<void> {
	const response = await authorizedFetch('/v1/auth/register', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ email, password })
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
}

export async function failure(response: Response): Promise<string> {
	const text = await response.text();
	return text || `request failed (${response.status})`;
}
