import { z } from 'zod';

import { authorizedFetch, failure } from './session';

export const notificationSchema = z.object({
	id: z.string(),
	project_id: z.string(),
	event_kind: z.string(),
	object: z.string().nullable().optional(),
	feed_seq: z.number().nullable().optional(),
	created_at: z.number(),
	read: z.boolean(),
});

export type Notification = z.infer<typeof notificationSchema>;

export async function notifications(unread = false): Promise<Notification[]> {
	const response = await authorizedFetch(`/v1/notifications?unread=${unread}`);
	if (response.status === 401) {
		throw new Error('sign in to see your notifications');
	}
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	return z.array(notificationSchema).parse(await response.json());
}

export async function readNotification(id: string): Promise<void> {
	const response = await authorizedFetch(`/v1/notifications/${encodeURIComponent(id)}/read`, {
		method: 'POST',
	});
	if (!response.ok) {
		throw new Error(await failure(response));
	}
}

export async function readAll(): Promise<void> {
	const response = await authorizedFetch('/v1/notifications/read-all', { method: 'POST' });
	if (!response.ok) {
		throw new Error(await failure(response));
	}
}

export async function follows(): Promise<string[]> {
	const response = await authorizedFetch('/v1/follows');
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	return z.array(z.string()).parse(await response.json());
}
