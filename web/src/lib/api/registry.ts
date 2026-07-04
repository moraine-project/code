import { z } from 'zod';

export const projectSummarySchema = z.object({
	project_id: z.string(),
	genesis: z.string(),
	head_seq: z.number(),
	head_entry: z.string().nullable().optional(),
	profile: z.string().nullable().optional()
});

export const linkSchema = z.object({
	kind: z.string(),
	url: z.string()
});

export const profileSchema = z.object({
	project_id: z.string(),
	display_name: z.string(),
	summary: z.string(),
	description: z.string(),
	categories: z.array(z.string()),
	tags: z.array(z.string()),
	links: z.array(linkSchema),
	communities: z.array(linkSchema),
	revision: z.string()
});

export const feedEntrySchema = z.object({
	seq: z.number(),
	kind: z.string(),
	title: z.string().nullable().optional(),
	object: z.string(),
	entry: z.string(),
	declared_at: z.number(),
	previous: z.string().nullable().optional()
});

export const feedPageSchema = z.object({
	project_id: z.string(),
	head_seq: z.number(),
	entries: z.array(feedEntrySchema),
	next: z.number().nullable().optional()
});

export type ProjectSummary = z.infer<typeof projectSummarySchema>;
export type Profile = z.infer<typeof profileSchema>;
export type FeedEntry = z.infer<typeof feedEntrySchema>;
export type FeedPage = z.infer<typeof feedPageSchema>;

export type Fetcher = typeof fetch;

export function normalizeBase(base: string): string {
	const trimmed = base.trim().replace(/\/+$/, '');
	if (trimmed.length === 0) {
		throw new Error('a home registry URL is required');
	}
	return trimmed;
}

export async function fetchProject(base: string, projectId: string, fetchFn: Fetcher = fetch): Promise<ProjectSummary> {
	const response = await fetchFn(`${normalizeBase(base)}/v1/projects/${encodeURIComponent(projectId)}`);
	if (!response.ok) {
		throw new Error(`home returned ${response.status} for the project`);
	}
	return projectSummarySchema.parse(await response.json());
}

export async function fetchProfile(
	base: string,
	projectId: string,
	fetchFn: Fetcher = fetch
): Promise<Profile | null> {
	const response = await fetchFn(`${normalizeBase(base)}/v1/projects/${encodeURIComponent(projectId)}/profile`);
	if (response.status === 404) {
		return null;
	}
	if (!response.ok) {
		throw new Error(`home returned ${response.status} for the profile`);
	}
	return profileSchema.parse(await response.json());
}

export async function fetchFeed(
	base: string,
	projectId: string,
	after = 0,
	limit = 50,
	fetchFn: Fetcher = fetch
): Promise<FeedPage> {
	const response = await fetchFn(
		`${normalizeBase(base)}/v1/projects/${encodeURIComponent(projectId)}/feed?after=${after}&limit=${limit}`
	);
	if (!response.ok) {
		throw new Error(`home returned ${response.status} for the feed`);
	}
	return feedPageSchema.parse(await response.json());
}

export async function fetchObject(base: string, objectId: string, fetchFn: Fetcher = fetch): Promise<Uint8Array> {
	const hex = objectId.startsWith('gd:sha256:') ? objectId.slice('gd:sha256:'.length) : objectId;
	const response = await fetchFn(`${normalizeBase(base)}/v1/objects/${hex}`);
	if (!response.ok) {
		throw new Error(`home returned ${response.status} for the object`);
	}
	return new Uint8Array(await response.arrayBuffer());
}

export function shortDigest(id: string, length = 12): string {
	const hex = id.startsWith('gd:sha256:') ? id.slice('gd:sha256:'.length) : id;
	if (hex.length <= length) {
		return hex;
	}
	return `${hex.slice(0, length)}…`;
}
