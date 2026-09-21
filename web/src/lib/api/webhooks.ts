import { z } from 'zod';
import { authorizedFetch, failure } from './session';

const webhookSchema = z.object({
	id: z.string(),
	url: z.string(),
	event_kinds: z.array(z.string()),
	created_at: z.number(),
});
export type Webhook = z.infer<typeof webhookSchema>;

export async function webhooks(): Promise<Webhook[]> {
	const response = await authorizedFetch('/v1/webhooks');
	if (!response.ok) throw new Error(await failure(response));
	return z.array(webhookSchema).parse(await response.json());
}

export async function createWebhook(url: string, eventKinds: string[]): Promise<void> {
	const response = await authorizedFetch('/v1/webhooks', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ url, event_kinds: eventKinds }),
	});
	if (!response.ok) throw new Error(await failure(response));
}

export async function revokeWebhook(id: string): Promise<void> {
	const response = await authorizedFetch(`/v1/webhooks/${encodeURIComponent(id)}`, {
		method: 'DELETE',
	});
	if (!response.ok) throw new Error(await failure(response));
}
