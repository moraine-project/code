import { z } from 'zod';

import { authorizedFetch, failure } from './session';

export const subscriptionSchema = z.object({
	home_url: z.string(),
	project_id: z.string(),
	cursor_seq: z.number(),
	lag_entries: z.number(),
	resets: z.number(),
	status: z.string(),
	updated_at: z.number(),
});

export type Subscription = z.infer<typeof subscriptionSchema>;

export const definitionSubscriptionSchema = z.object({
	home_url: z.string(),
	id: z.string(),
	kind: z.string(),
	updated_at: z.number(),
});
export type DefinitionSubscription = z.infer<typeof definitionSubscriptionSchema>;

const witnessObservationSchema = z.object({
	observer_id: z.string(),
	source_home: z.string(),
	sequence: z.number(),
	head_entry: z.string(),
	observed_at: z.number(),
});

const witnessConflictSchema = z.object({
	sequence: z.number(),
	entries: z.array(z.string()),
	homes: z.array(z.string()),
	observers: z.array(z.string()),
});

export const witnessReportSchema = z.object({
	project_id: z.string(),
	observations: z.array(witnessObservationSchema),
	conflicts: z.array(witnessConflictSchema),
});
export type WitnessReport = z.infer<typeof witnessReportSchema>;

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

export async function witness(projectId: string): Promise<WitnessReport> {
	const response = await authorizedFetch(`/v1/projects/${encodeURIComponent(projectId)}/witness`);
	if (!response.ok) throw new Error(await failure(response));
	return witnessReportSchema.parse(await response.json());
}

export async function definitionSubscriptions(): Promise<DefinitionSubscription[]> {
	const response = await authorizedFetch('/v1/definition-subscriptions');
	if (!response.ok) throw new Error(await failure(response));
	return z.array(definitionSubscriptionSchema).parse(await response.json());
}

export async function unfollow(homeUrl: string, projectId: string): Promise<void> {
	const params = new URLSearchParams({ home_url: homeUrl, project_id: projectId });
	const response = await authorizedFetch(`/v1/subscriptions?${params.toString()}`, {
		method: 'DELETE',
	});
	if (!response.ok) {
		throw new Error(await failure(response));
	}
}

export async function follow(homeUrl: string, projectId: string): Promise<void> {
	const response = await authorizedFetch('/v1/federation/sync', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ home_url: homeUrl, project_id: projectId }),
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
		cursor: String(cursor),
	});
	const response = await authorizedFetch(`/v1/subscriptions/reset?${params.toString()}`, {
		method: 'POST',
	});
	if (!response.ok) {
		throw new Error(await failure(response));
	}
}

export async function syncDefinition(
	homeUrl: string,
	id: string,
	kind: 'game' | 'loader' | 'runtime',
): Promise<{ id: string; kind: string; definition: string }> {
	const response = await authorizedFetch('/v1/federation/sync-definition', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ home_url: homeUrl, id, kind }),
	});
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	return z
		.object({ id: z.string(), kind: z.string(), definition: z.string() })
		.parse(await response.json());
}

export async function subscribeDefinition(
	homeUrl: string,
	id: string,
	kind: 'game' | 'loader' | 'runtime',
): Promise<{ id: string; kind: string; definition: string }> {
	const response = await authorizedFetch('/v1/federation/subscribe-definition', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ home_url: homeUrl, id, kind }),
	});
	if (!response.ok) throw new Error(await failure(response));
	return z
		.object({ id: z.string(), kind: z.string(), definition: z.string() })
		.parse(await response.json());
}

export async function pinMirror(mirrorId: string, publicKey: string): Promise<string> {
	const response = await authorizedFetch(`/v1/mirrors/${encodeURIComponent(mirrorId)}/keys`, {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ public_key: publicKey }),
	});
	if (!response.ok) throw new Error(await failure(response));
	return z.object({ mirror_id: z.string() }).parse(await response.json()).mirror_id;
}
