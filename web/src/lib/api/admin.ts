import { z } from 'zod';

import { authorizedFetch, failure } from './session';

export const overviewSchema = z.object({
	projects: z.number(),
	definitions: z.number(),
	accounts: z.number(),
	unverified_accounts: z.number(),
	pending_submissions: z.number(),
	followed_homes: z.number(),
	advisories: z.number(),
});

export type Overview = z.infer<typeof overviewSchema>;

export async function overview(): Promise<Overview> {
	const response = await authorizedFetch('/v1/admin/overview');
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	return overviewSchema.parse(await response.json());
}

export const adminAccountSchema = z.object({
	user_id: z.string(),
	email: z.string(),
	role: z.string(),
	created_at: z.number(),
	verified: z.boolean(),
});

export type AdminAccount = z.infer<typeof adminAccountSchema>;

export async function accounts(): Promise<AdminAccount[]> {
	const response = await authorizedFetch('/v1/auth/users');
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	return z.array(adminAccountSchema).parse(await response.json());
}

export async function createAccount(
	email: string,
	role: string,
): Promise<{ user_id: string; password: string }> {
	const response = await authorizedFetch('/v1/auth/users', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ email, role }),
	});
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	return z.object({ user_id: z.string(), password: z.string() }).parse(await response.json());
}

export async function deleteAccount(userId: string): Promise<void> {
	const response = await authorizedFetch(`/v1/auth/users/${encodeURIComponent(userId)}`, {
		method: 'DELETE',
	});
	if (!response.ok) {
		throw new Error(await failure(response));
	}
}

export async function resetPassword(email: string): Promise<string> {
	const response = await authorizedFetch('/v1/auth/users/reset-password', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ email }),
	});
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	const body = (await response.json()) as { password?: string };
	return body.password ?? '';
}
