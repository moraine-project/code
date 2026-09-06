import { z } from 'zod';

import { authorizedFetch } from './session';

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

export const lookupMatchSchema = z.object({
	project_id: z.string(),
	release: z.string(),
	human_version: z.string().nullable().optional(),
	filename: z.string().nullable().optional(),
});

export const digestLookupSchema = z.object({
	digest: z.string(),
	matches: z.array(lookupMatchSchema),
});

export const artifactSchema = z.object({
	digest: z.string(),
	size: z.number(),
	media_type: z.string(),
	filename: z.string(),
	is_primary: z.boolean(),
});

export const compatibilitySchema = z.object({
	scheme: z.string(),
	values: z.array(z.string()),
	loader_id: z.string().nullable().optional(),
	side: z.string(),
});

export const dependencySchema = z.object({
	target_kind: z.string(),
	target_id: z.string(),
	kind: z.string(),
});

export const rightsSchema = z.object({
	redistribution: z.string(),
	modpack_inclusion: z.string(),
	mirroring: z.string(),
	attribution_required: z.boolean(),
});

export const withdrawalSchema = z.object({
	reason: z.string(),
	note: z.string().nullable().optional(),
	declared_time: z.number(),
});

export const advisorySchema = z.object({
	advisory: z.string(),
	provider_id: z.string(),
	project_id: z.string(),
	severity: z.string(),
	category: z.string(),
	block_promotion: z.boolean(),
	affected_digest: z.string().nullable().optional(),
	published_at: z.number(),
	retracted_at: z.number().nullable().optional(),
});

export const attestationSchema = z.object({
	attestation: z.string(),
	kind: z.string(),
	signer_id: z.string(),
	subject_kind: z.string(),
	subject_id: z.string(),
	media_type: z.string(),
	issued_at: z.number(),
	body_digest: z.string().nullable().optional(),
	has_inline_body: z.boolean(),
});

export const releaseSchema = z.object({
	project_id: z.string(),
	human_version: z.string(),
	channel: z.string(),
	kind: z.string(),
	declared_time: z.number(),
	license_expression: z.string().nullable().optional(),
	artifacts: z.array(artifactSchema),
	compatibility: z.array(compatibilitySchema),
	dependencies: z.array(dependencySchema),
	rights: rightsSchema.nullable().optional(),
	withdrawal: withdrawalSchema.nullable().optional(),
	advisories: z.array(advisorySchema).optional(),
	attestations: z.array(attestationSchema).optional(),
	changelog: z.string().nullable().optional(),
});

export const changelogSchema = z.object({
	project_id: z.string(),
	release: z.string().nullable().optional(),
	locale_sections: z.array(
		z.object({
			locale: z.string(),
			sections: z.array(
				z.object({
					heading: z.string(),
					body: z.string(),
					severity: z.string().nullable().optional(),
				}),
			),
		}),
	),
	declared_time: z.number(),
});

export const searchResultSchema = z.object({
	project_id: z.string(),
	game_id: z.string(),
	display_name: z.string(),
	summary: z.string(),
	icon_url: z.string().nullable().optional(),
	listing_state: z.string(),
	source_instance: z.string(),
	home: z.string().nullable().optional(),
	annotations: z.array(z.object({ kind: z.string(), label: z.string() })).optional(),
	instance_popularity: z.object({ window: z.string(), value: z.number() }).nullable().optional(),
});

export const searchResponseSchema = z.object({
	protocol: z.number(),
	results: z.array(searchResultSchema),
	next_cursor: z.string().nullable().optional(),
	total_estimate: z.number().nullable().optional(),
});

export const facetValueSchema = z.object({
	value: z.string(),
	count: z.number(),
});

export const facetsSchema = z.object({
	game: z.array(facetValueSchema),
	loader: z.array(facetValueSchema),
	category: z.array(facetValueSchema),
	tag: z.array(facetValueSchema),
	game_version: z.array(facetValueSchema),
	loader_version: z.array(facetValueSchema),
	runtime_version: z.array(facetValueSchema),
	channel: z.array(facetValueSchema),
	platform: z.array(facetValueSchema),
});

export type Facets = z.infer<typeof facetsSchema>;
export type FacetValue = z.infer<typeof facetValueSchema>;

export const emptyFacets: Facets = {
	game: [],
	loader: [],
	category: [],
	tag: [],
	game_version: [],
	loader_version: [],
	runtime_version: [],
	channel: [],
	platform: [],
};

export const definitionSummarySchema = z.object({
	id: z.string(),
	kind: z.string(),
	current: z.string().nullable().optional(),
	display_name: z.string().nullable().optional(),
});

export type ProjectSummary = z.infer<typeof projectSummarySchema>;
export type DefinitionSummary = z.infer<typeof definitionSummarySchema>;

export const definitionDetailSchema = definitionSummarySchema.extend({
	genesis: z.string(),
	current: z.string(),
	payload: z.unknown(),
});

export type DefinitionDetail = z.infer<typeof definitionDetailSchema>;

export async function fetchDefinition(
	base: string,
	kind: 'games' | 'loaders' | 'runtimes',
	id: string,
	fetchFn: Fetcher = fetch,
): Promise<DefinitionDetail | null> {
	const response = await fetchFn(`${normalizeBase(base)}/v1/${kind}/${encodeURIComponent(id)}`);
	if (response.status === 404) {
		return null;
	}
	if (!response.ok) {
		throw new Error(`home returned ${response.status} for the definition`);
	}
	return definitionDetailSchema.parse(await response.json());
}
export type Profile = z.infer<typeof profileSchema>;
export type DigestLookup = z.infer<typeof digestLookupSchema>;
export type SearchResult = z.infer<typeof searchResultSchema>;
export type Release = z.infer<typeof releaseSchema>;
export type Changelog = z.infer<typeof changelogSchema>;
export type Attestation = z.infer<typeof attestationSchema>;
export type Artifact = z.infer<typeof artifactSchema>;
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

export async function fetchProject(
	base: string,
	projectId: string,
	fetchFn: Fetcher = fetch,
): Promise<ProjectSummary> {
	const response = await fetchFn(
		`${normalizeBase(base)}/v1/projects/${encodeURIComponent(projectId)}`,
	);
	if (!response.ok) {
		throw new Error(`home returned ${response.status} for the project`);
	}
	return projectSummarySchema.parse(await response.json());
}

export async function fetchProfile(
	base: string,
	projectId: string,
	fetchFn: Fetcher = fetch,
): Promise<Profile | null> {
	const response = await fetchFn(
		`${normalizeBase(base)}/v1/projects/${encodeURIComponent(projectId)}/profile`,
	);
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
		`${normalizeBase(base)}/v1/projects/${encodeURIComponent(projectId)}/feed?${params.toString()}`,
	);
	if (!response.ok) {
		throw new Error(`home returned ${response.status} for the feed`);
	}
	return feedPageSchema.parse(await response.json());
}

export async function fetchObject(
	base: string,
	objectId: string,
	fetchFn: Fetcher = fetch,
): Promise<Uint8Array> {
	const hex = objectId.startsWith('gd:sha256:') ? objectId.slice('gd:sha256:'.length) : objectId;
	const response = await fetchFn(`${normalizeBase(base)}/v1/objects/${hex}`);
	if (!response.ok) {
		throw new Error(`home returned ${response.status} for the object`);
	}
	return new Uint8Array(await response.arrayBuffer());
}

export async function lookupDigest(
	base: string,
	digest: string,
	fetchFn: Fetcher = fetch,
): Promise<DigestLookup> {
	const response = await fetchFn(
		`${normalizeBase(base)}/v1/lookup?sha256=${encodeURIComponent(digest.trim())}`,
	);
	if (!response.ok) {
		throw new Error(`home returned ${response.status} for the digest`);
	}
	return digestLookupSchema.parse(await response.json());
}

export type SearchQueryParams = {
	q?: string;
	game?: string;
	loader?: string;
	tag?: string;
	category?: string;
	gameVersion?: string;
	loaderVersion?: string;
	runtimeVersion?: string;
	channel?: string;
	platform?: string;
	state?: string;
	sort?: string;
	limit?: number;
};

function searchParams(query: SearchQueryParams): URLSearchParams {
	const params = new URLSearchParams();
	if (query.q) params.set('q', query.q);
	if (query.game) params.set('game', query.game);
	if (query.loader) params.set('loader', query.loader);
	if (query.tag) params.set('tag', query.tag);
	if (query.category) params.set('category', query.category);
	if (query.gameVersion) params.set('game_version', query.gameVersion);
	if (query.loaderVersion) params.set('loader_version', query.loaderVersion);
	if (query.runtimeVersion) params.set('runtime_version', query.runtimeVersion);
	if (query.channel) params.set('channel', query.channel);
	if (query.platform) params.set('platform', query.platform);
	if (query.state) params.set('state', query.state);
	if (query.sort) params.set('sort', query.sort);
	return params;
}

export async function searchFacets(
	base: string,
	query: SearchQueryParams,
	fetchFn: Fetcher = fetch,
): Promise<Facets> {
	const response = await fetchFn(
		`${normalizeBase(base)}/v1/search/facets?${searchParams(query).toString()}`,
	);
	if (!response.ok) {
		throw new Error(`home returned ${response.status} for the facets`);
	}
	return facetsSchema.parse((await response.json()).facets);
}

export async function searchProjects(
	base: string,
	query: SearchQueryParams,
	fetchFn: Fetcher = fetch,
): Promise<SearchResult[]> {
	const params = searchParams(query);
	params.set('limit', String(query.limit ?? 20));
	const response = await fetchFn(`${normalizeBase(base)}/v1/search?${params.toString()}`);
	if (!response.ok) {
		throw new Error(`home returned ${response.status} for the search`);
	}
	return searchResponseSchema.parse(await response.json()).results;
}

export async function listDefinitions(
	base: string,
	kind: 'games' | 'loaders' | 'runtimes',
	fetchFn: Fetcher = fetch,
): Promise<DefinitionSummary[]> {
	const response = await fetchFn(`${normalizeBase(base)}/v1/${kind}`);
	if (!response.ok) {
		throw new Error(`home returned ${response.status} for the ${kind}`);
	}
	return z.array(definitionSummarySchema).parse(await response.json());
}

export async function fetchRelease(
	base: string,
	projectId: string,
	hex: string,
	fetchFn: Fetcher = fetch,
): Promise<Release | null> {
	const response = await fetchFn(
		`${normalizeBase(base)}/v1/projects/${encodeURIComponent(projectId)}/releases/${hex}`,
	);
	if (response.status === 404) {
		return null;
	}
	if (!response.ok) {
		throw new Error(`home returned ${response.status} for the release`);
	}
	return releaseSchema.parse(await response.json());
}

export async function fetchChangelog(
	base: string,
	projectId: string,
	digest: string,
	fetchFn: Fetcher = fetch,
): Promise<Changelog | null> {
	const response = await fetchFn(
		`${normalizeBase(base)}/v1/projects/${encodeURIComponent(projectId)}/changelog/${digestHex(digest)}`,
	);
	if (response.status === 404) {
		return null;
	}
	if (!response.ok) {
		throw new Error(`home returned ${response.status} for the changelog`);
	}
	return changelogSchema.parse(await response.json());
}

export function blobUrl(base: string, digest: string): string {
	return `${normalizeBase(base)}/v1/blobs/sha256/${digestHex(digest)}`;
}

export function digestHex(digest: string): string {
	return digest.startsWith('sha256:') ? digest.slice('sha256:'.length) : digest;
}

export async function fileSha256(file: File): Promise<string> {
	// PERF: WebCrypto has no streaming digest, so large files buffer in memory; the CLI streams.
	const digest = await crypto.subtle.digest('SHA-256', await file.arrayBuffer());
	return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('');
}

export const uploadReceiptSchema = z.object({
	digest: z.string(),
	size: z.number(),
});

export type UploadReceipt = z.infer<typeof uploadReceiptSchema>;

export async function uploadBlob(file: File): Promise<UploadReceipt> {
	const response = await authorizedFetch('/v1/blobs', {
		method: 'POST',
		headers: { 'content-type': 'application/octet-stream' },
		body: file,
	});
	if (!response.ok) {
		throw new Error(`upload failed (${response.status})`);
	}
	return uploadReceiptSchema.parse(await response.json());
}

export async function publishingMode(fetchFn: Fetcher = fetch): Promise<string> {
	const response = await fetchFn('/.well-known/mod-registry');
	if (!response.ok) {
		return 'review';
	}
	const document = await response.json();
	return typeof document.publishing === 'string' ? document.publishing : 'review';
}

export function shortDigest(id: string, length = 12): string {
	const hex = id.startsWith('gd:sha256:') ? id.slice('gd:sha256:'.length) : id;
	if (hex.length <= length) {
		return hex;
	}
	return `${hex.slice(0, length)}…`;
}
