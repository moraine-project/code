import { z } from 'zod';

import type { Fetcher } from './request';
import { ApiError, registryUrl } from './request';

export const projectSummarySchema = z.object({
	project_id: z.string(),
	genesis: z.string(),
	head_seq: z.number(),
	head_entry: z.string().nullable().optional(),
	profile: z.string().nullable().optional(),
	listing_state: z.string().optional(),
	reason_code: z.string().nullable().optional(),
});

export const linkSchema = z.object({
	kind: z.string(),
	url: z.string(),
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
	revision: z.string(),
});

export const releaseSummarySchema = z.object({
	channel: z.string(),
	game_id: z.string(),
	loaders: z.array(z.string()),
});

export const feedEntrySchema = z.object({
	seq: z.number(),
	kind: z.string(),
	title: z.string().nullable().optional(),
	release: releaseSummarySchema.nullable().optional(),
	object: z.string(),
	entry: z.string(),
	declared_at: z.number(),
	previous: z.string().nullable().optional(),
});

export const feedPageSchema = z.object({
	project_id: z.string(),
	head_seq: z.number(),
	entries: z.array(feedEntrySchema),
	next: z.number().nullable().optional(),
	truncated: z.boolean().optional(),
});

export const channelSchema = z.object({
	channel: z.string(),
	release: z.string(),
	human_version: z.string(),
	seq: z.number(),
});

export const migrationSchema = z.object({
	migration: z.string(),
	old_home: z.string(),
	new_home: z.string(),
	cutover_seq: z.number(),
	reason: z.string().nullable().optional(),
	declared_time: z.number(),
});

export const recoverySchema = z.object({
	project_id: z.string(),
	threshold: z.number(),
	roots: z.array(z.object({ key_id: z.string(), public_key: z.string() })),
	valid_from_seq: z.number().nullable().optional(),
	recovered: z.boolean(),
	claims: z.array(
		z.object({
			claim: z.string(),
			valid_from_seq: z.number(),
			applied: z.boolean(),
			roots: z.array(z.object({ key_id: z.string(), public_key: z.string() })),
		}),
	),
});

export const denyEntrySchema = z.object({
	issuer_id: z.string(),
	target_kind: z.string(),
	target_id: z.string(),
	reason_code: z.string(),
	reason_taxonomy_version: z.number(),
	scope_kind: z.string(),
	scope_id: z.string(),
	valid_from: z.number().nullable().optional(),
	valid_until: z.number().nullable().optional(),
	deny_list: z.string(),
});

export type ProjectSummary = z.infer<typeof projectSummarySchema>;
export type Profile = z.infer<typeof profileSchema>;
export type FeedEntry = z.infer<typeof feedEntrySchema>;
export type FeedPage = z.infer<typeof feedPageSchema>;
export type Channel = z.infer<typeof channelSchema>;
export type Migration = z.infer<typeof migrationSchema>;
export type Recovery = z.infer<typeof recoverySchema>;
export type DenyEntry = z.infer<typeof denyEntrySchema>;

export async function fetchProject(
	base: string,
	projectId: string,
	fetchFn: Fetcher = fetch,
): Promise<ProjectSummary> {
	const response = await fetchFn(
		registryUrl(base, `/v1/projects/${encodeURIComponent(projectId)}`),
	);
	if (!response.ok) {
		throw new ApiError(response.status, `home returned ${response.status} for the project`);
	}
	return projectSummarySchema.parse(await response.json());
}

export async function fetchProfile(
	base: string,
	projectId: string,
	fetchFn: Fetcher = fetch,
): Promise<Profile | null> {
	const response = await fetchFn(
		registryUrl(base, `/v1/projects/${encodeURIComponent(projectId)}/profile`),
	);
	if (response.status === 404) {
		return null;
	}
	if (!response.ok) {
		throw new ApiError(response.status, `home returned ${response.status} for the profile`);
	}
	return profileSchema.parse(await response.json());
}

export async function fetchFeed(
	base: string,
	projectId: string,
	after = 0,
	limit = 50,
	filters: { gameVersion?: string; loader?: string; loaderVersion?: string } = {},
	fetchFn: Fetcher = fetch,
): Promise<FeedPage> {
	const params = new URLSearchParams({ after: String(after), limit: String(limit) });
	if (filters.gameVersion) {
		params.set('game_version', filters.gameVersion);
	}
	if (filters.loader) {
		params.set('loader', filters.loader);
	}
	if (filters.loaderVersion) {
		params.set('loader_version', filters.loaderVersion);
	}
	const response = await fetchFn(
		registryUrl(base, `/v1/projects/${encodeURIComponent(projectId)}/feed?${params.toString()}`),
	);
	if (!response.ok) {
		throw new ApiError(response.status, `home returned ${response.status} for the feed`);
	}
	return feedPageSchema.parse(await response.json());
}

export async function fetchObject(
	base: string,
	objectId: string,
	fetchFn: Fetcher = fetch,
): Promise<Uint8Array> {
	const hex = objectId.startsWith('gd:sha256:') ? objectId.slice('gd:sha256:'.length) : objectId;
	const response = await fetchFn(registryUrl(base, `/v1/objects/${hex}`));
	if (!response.ok) {
		throw new ApiError(response.status, `home returned ${response.status} for the object`);
	}
	return new Uint8Array(await response.arrayBuffer());
}

export async function fetchChannels(
	base: string,
	projectId: string,
	fetchFn: Fetcher = fetch,
): Promise<Channel[]> {
	const response = await fetchFn(
		registryUrl(base, `/v1/projects/${encodeURIComponent(projectId)}/channels`),
	);
	if (!response.ok)
		throw new ApiError(response.status, `home returned ${response.status} for channels`);
	return z.array(channelSchema).parse(await response.json());
}

export async function fetchMigrations(
	base: string,
	projectId: string,
	fetchFn: Fetcher = fetch,
): Promise<Migration[]> {
	const response = await fetchFn(
		registryUrl(base, `/v1/projects/${encodeURIComponent(projectId)}/migrations`),
	);
	if (!response.ok)
		throw new ApiError(response.status, `home returned ${response.status} for migrations`);
	return z.array(migrationSchema).parse(await response.json());
}

export async function fetchRecovery(
	base: string,
	projectId: string,
	fetchFn: Fetcher = fetch,
): Promise<Recovery> {
	const response = await fetchFn(
		registryUrl(base, `/v1/projects/${encodeURIComponent(projectId)}/recovery`),
	);
	if (!response.ok)
		throw new ApiError(response.status, `home returned ${response.status} for recovery state`);
	return recoverySchema.parse(await response.json());
}

export async function fetchDenyEntries(
	base: string,
	projectId: string,
	fetchFn: Fetcher = fetch,
): Promise<DenyEntry[]> {
	const response = await fetchFn(
		registryUrl(base, `/v1/deny-lists?project=${encodeURIComponent(projectId)}`),
	);
	if (!response.ok)
		throw new ApiError(response.status, `home returned ${response.status} for deny-list entries`);
	return z.array(denyEntrySchema).parse(await response.json());
}
