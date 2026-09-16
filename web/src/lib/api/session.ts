import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';
import { z } from 'zod';

export const accountSchema = z.object({
	user_id: z.string(),
	email: z.string(),
	via: z.string(),
	role: z.string().optional(),
	verified: z.boolean().optional(),
});

export type Account = z.infer<typeof accountSchema>;

let csrfInMemory: string | null = null;

export function setCsrfToken(token: string | null) {
	csrfInMemory = token;
}

export function isLoopbackHost(host: string): boolean {
	return host === 'localhost' || host === '127.0.0.1' || host === '[::1]' || host === '::1';
}

export function sameOrigin(configured: string, origin: string): boolean {
	if (configured === origin) {
		return true;
	}
	try {
		const base = new URL(configured);
		const page = new URL(origin);
		return (
			base.port === page.port && isLoopbackHost(base.hostname) && isLoopbackHost(page.hostname)
		);
	} catch {
		return false;
	}
}

export function apiBase(): string {
	const configured = (PUBLIC_MORAINE_REGISTRY ?? '').replace(/\/+$/, '');
	if (!configured) {
		return '';
	}
	if (typeof window !== 'undefined' && sameOrigin(configured, window.location.origin)) {
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
	try {
		return await fetch(`${base}${path}`, {
			credentials: base ? 'include' : 'same-origin',
			...init,
			headers,
		});
	} catch (cause) {
		if (base) {
			throw new Error(
				`could not reach ${base}. If this site is served from another origin, add that origin to MORAINE_WEB_ORIGINS on the registry.`,
				{ cause },
			);
		}
		throw cause;
	}
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

export async function changePassword(current: string, next: string): Promise<void> {
	const response = await authorizedFetch('/v1/auth/password', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ current, new: next }),
	});
	if (!response.ok) {
		throw new Error(await failure(response));
	}
}

export async function issueRecoveryCodes(): Promise<string[]> {
	const response = await authorizedFetch('/v1/auth/recovery-codes', { method: 'POST' });
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	const body = (await response.json()) as { codes?: string[] };
	return body.codes ?? [];
}

export async function recover(email: string, code: string, next: string): Promise<void> {
	const response = await authorizedFetch('/v1/auth/recover', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ email, code, new: next }),
	});
	if (!response.ok) {
		throw new Error(await failure(response));
	}
}

export async function exportAccount(): Promise<unknown> {
	const response = await authorizedFetch('/v1/auth/export');
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	return response.json();
}

export async function deleteAccount(): Promise<void> {
	const response = await authorizedFetch('/v1/auth/me', { method: 'DELETE' });
	if (!response.ok) {
		throw new Error(await failure(response));
	}
}

export async function verifyEmail(token: string): Promise<void> {
	const response = await authorizedFetch('/v1/auth/verify-email', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ token }),
	});
	if (!response.ok) {
		throw new Error(await failure(response));
	}
}

export async function resendVerification(): Promise<void> {
	const response = await authorizedFetch('/v1/auth/verify-email/resend', { method: 'POST' });
	if (!response.ok) {
		throw new Error(await failure(response));
	}
}

export async function failure(response: Response): Promise<string> {
	const text = await response.text();
	return text || `request failed (${response.status})`;
}
