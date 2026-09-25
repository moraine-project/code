import { z } from 'zod';

import type { Fetcher } from './request';
import { ApiError, registryUrl } from './request';

export const definitionSummarySchema = z.object({
	id: z.string(),
	kind: z.string(),
	current: z.string().nullable().optional(),
	display_name: z.string().nullable().optional(),
	source_home: z.string().nullable().optional(),
});

export const definitionDetailSchema = definitionSummarySchema.extend({
	genesis: z.string(),
	current: z.string(),
	payload: z.unknown(),
});

export const vocabularyEntrySchema = z.object({
	id: z.string(),
	label: z.string(),
	parent: z.string().nullable().optional(),
});

export const gamePayloadSchema = z.object({
	game_id: z.string(),
	display_name: z.string(),
	version_ordering: z.string().optional(),
	version_catalog: z.array(z.string()).optional(),
	categories: z.array(vocabularyEntrySchema).optional(),
	tags: z.array(vocabularyEntrySchema).optional(),
	loaders_allowed: z.boolean().optional(),
});

export type DefinitionSummary = z.infer<typeof definitionSummarySchema>;
export type DefinitionDetail = z.infer<typeof definitionDetailSchema>;
export type GamePayload = z.infer<typeof gamePayloadSchema>;

export const DEFINITION_KINDS = ['games', 'loaders', 'runtimes'] as const;
export type DefinitionKind = (typeof DEFINITION_KINDS)[number];

const ROUTE_SEGMENTS: Record<string, DefinitionKind> = {
	game: 'games',
	loader: 'loaders',
	runtime: 'runtimes',
};

export function definitionRouteSegment(kind: string): DefinitionKind | null {
	return ROUTE_SEGMENTS[kind] ?? null;
}

export async function fetchDefinition(
	base: string,
	kind: DefinitionKind,
	id: string,
	fetchFn: Fetcher = fetch,
): Promise<DefinitionDetail | null> {
	const response = await fetchFn(registryUrl(base, `/v1/${kind}/${encodeURIComponent(id)}`));
	if (response.status === 404) {
		return null;
	}
	if (!response.ok) {
		throw new ApiError(response.status, `home returned ${response.status} for the definition`);
	}
	return definitionDetailSchema.parse(await response.json());
}

export async function listDefinitions(
	base: string,
	kind: DefinitionKind,
	fetchFn: Fetcher = fetch,
): Promise<DefinitionSummary[]> {
	const response = await fetchFn(registryUrl(base, `/v1/${kind}`));
	if (!response.ok) {
		throw new ApiError(response.status, `home returned ${response.status} for the ${kind}`);
	}
	return z.array(definitionSummarySchema).parse(await response.json());
}

export async function listLoaders(
	base: string,
	game?: string,
	fetchFn: Fetcher = fetch,
): Promise<DefinitionSummary[]> {
	const params = new URLSearchParams();
	if (game) {
		params.set('game', game);
	}
	const suffix = params.toString() ? `?${params.toString()}` : '';
	const response = await fetchFn(registryUrl(base, `/v1/loaders${suffix}`));
	if (!response.ok) {
		throw new ApiError(response.status, `home returned ${response.status} for the loaders`);
	}
	return z.array(definitionSummarySchema).parse(await response.json());
}

export async function fetchGamePayload(
	base: string,
	gameId: string,
	fetchFn: Fetcher = fetch,
): Promise<GamePayload | null> {
	const definition = await fetchDefinition(base, 'games', gameId, fetchFn);
	if (!definition) {
		return null;
	}
	return gamePayloadSchema.parse(definition.payload);
}
