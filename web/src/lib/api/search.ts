import { z } from 'zod';

import type { Fetcher } from './request';
import { registryUrl } from './request';

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
export type SearchResult = z.infer<typeof searchResultSchema>;

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
		registryUrl(base, `/v1/search/facets?${searchParams(query).toString()}`),
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
	const response = await fetchFn(registryUrl(base, `/v1/search?${params.toString()}`));
	if (!response.ok) {
		throw new Error(`home returned ${response.status} for the search`);
	}
	return searchResponseSchema.parse(await response.json()).results;
}
