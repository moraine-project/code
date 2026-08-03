import { z } from 'zod';

import { authorizedFetch, failure } from './session';

export const subscriptionSchema = z.object({
	home_url: z.string(),
	project_id: z.string(),
	cursor_seq: z.number(),
	lag_entries: z.number(),
	status: z.string(),
	updated_at: z.number()
});

export type Subscription = z.infer<typeof subscriptionSchema>;

export async function subscriptions(): Promise<Subscription[]> {
	const response = await authorizedFetch('/v1/subscriptions');
	if (response.status === 401 || response.status === 403) {
		throw new Error('sign in with an account that manages federation');
	}
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	return z.array(subscriptionSchema).parse(await response.json());
}

export async function unfollow(homeUrl: string, projectId: string): Promise<void> {
	const params = new URLSearchParams({ home_url: homeUrl, project_id: projectId });
	const response = await authorizedFetch(`/v1/subscriptions?${params.toString()}`, { method: 'DELETE' });
	if (!response.ok) {
		throw new Error(await failure(response));
	}
}

export async function follow(homeUrl: string, projectId: string): Promise<void> {
	const response = await authorizedFetch('/v1/federation/sync', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ home_url: homeUrl, project_id: projectId })
	});
	if (!response.ok) {
		throw new Error(await failure(response));
	}
}

export async function resync(): Promise<{ synced: number; failed: number }> {
	const response = await authorizedFetch('/v1/federation/resync', { method: 'POST' });
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	return z.object({ synced: z.number(), failed: z.number() }).parse(await response.json());
}

export async function resetCursor(homeUrl: string, projectId: string, cursor = 0): Promise<void> {
	const params = new URLSearchParams({
		home_url: homeUrl,
		project_id: projectId,
		cursor: String(cursor)
	});
	const response = await authorizedFetch(`/v1/subscriptions/reset?${params.toString()}`, { method: 'POST' });
	if (!response.ok) {
		throw new Error(await failure(response));
	}
}
