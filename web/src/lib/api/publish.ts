import { z } from 'zod';

import { authorizedFetch } from './session';

export const projectReceiptSchema = z.object({
	project_id: z.string(),
	genesis: z.string(),
});

export const objectReceiptSchema = z.object({
	id: z.string(),
	kind: z.string(),
});

export const feedReceiptSchema = z.object({
	seq: z.number(),
	entry: z.string(),
});

export const submissionReceiptSchema = z.object({
	id: z.string(),
	state: z.string(),
	seq: z.number().nullable().optional(),
});

export type ProjectReceipt = z.infer<typeof projectReceiptSchema>;
export type ObjectReceipt = z.infer<typeof objectReceiptSchema>;
export type FeedReceipt = z.infer<typeof feedReceiptSchema>;
export type SubmissionReceipt = z.infer<typeof submissionReceiptSchema>;

async function failure(response: Response): Promise<Error> {
	const text = await response.text();
	return new Error(text || `request failed (${response.status})`);
}

async function postWire(path: string, wire: Uint8Array): Promise<Response> {
	return authorizedFetch(path, {
		method: 'POST',
		headers: { 'content-type': 'application/octet-stream' },
		body: wire as BodyInit,
	});
}

export async function createProject(wire: Uint8Array): Promise<ProjectReceipt> {
	const response = await postWire('/v1/projects', wire);
	if (!response.ok) {
		throw await failure(response);
	}
	return projectReceiptSchema.parse(await response.json());
}

export async function storeObject(
	projectId: string,
	kind: string,
	wire: Uint8Array,
): Promise<ObjectReceipt> {
	const response = await postWire(`/v1/projects/${projectId}/objects/${kind}`, wire);
	if (!response.ok) {
		throw await failure(response);
	}
	return objectReceiptSchema.parse(await response.json());
}

export async function appendFeed(projectId: string, wire: Uint8Array): Promise<FeedReceipt> {
	const response = await postWire(`/v1/projects/${projectId}/feed`, wire);
	if (!response.ok) {
		throw await failure(response);
	}
	return feedReceiptSchema.parse(await response.json());
}

export async function transferProject(
	projectId: string,
	wire: Uint8Array,
): Promise<{ transfer: string }> {
	const response = await postWire(`/v1/projects/${projectId}/transfer`, wire);
	if (!response.ok) throw await failure(response);
	return z.object({ transfer: z.string() }).parse(await response.json());
}

export async function submitFeed(wire: Uint8Array): Promise<SubmissionReceipt> {
	const response = await authorizedFetch('/v1/submissions', {
		method: 'POST',
		headers: { 'content-type': 'application/octet-stream' },
		body: wire as BodyInit,
	});
	if (!response.ok) {
		throw await failure(response);
	}
	return submissionReceiptSchema.parse(await response.json());
}
