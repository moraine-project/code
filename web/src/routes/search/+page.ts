import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';
import {
	emptyFacets,
	listDefinitions,
	normalizeBase,
	searchFacets,
	searchProjects,
} from '$lib/api/registry';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ url, fetch }) => {
	const home = url.searchParams.get('home') ?? PUBLIC_MORAINE_REGISTRY ?? '';
	const q = url.searchParams.get('q') ?? '';
	const game = url.searchParams.get('game') ?? '';
	const loader = url.searchParams.get('loader') ?? '';
	const category = url.searchParams.get('category') ?? '';
	const tag = url.searchParams.get('tag') ?? '';
	const gameVersion = url.searchParams.get('game_version') ?? '';
	const loaderVersion = url.searchParams.get('loader_version') ?? '';
	const runtimeVersion = url.searchParams.get('runtime_version') ?? '';
	const channel = url.searchParams.get('channel') ?? '';
	const platform = url.searchParams.get('platform') ?? '';
	const sort = url.searchParams.get('sort') ?? '';
	const filters = {
		home,
		q,
		game,
		loader,
		category,
		tag,
		gameVersion,
		loaderVersion,
		runtimeVersion,
		channel,
		platform,
		sort,
	};
	const anyFilter = [
		q,
		game,
		loader,
		category,
		tag,
		gameVersion,
		loaderVersion,
		runtimeVersion,
		channel,
		platform,
	].some((value) => value.trim().length > 0);
	try {
		const base = normalizeBase(home);
		const query = {
			q,
			game,
			loader,
			category,
			tag,
			gameVersion,
			loaderVersion,
			runtimeVersion,
			channel,
			platform,
			sort,
			limit: 20,
		};
		const [games, loaders, facets] = await Promise.all([
			listDefinitions(base, 'games', fetch).catch(() => []),
			listDefinitions(base, 'loaders', fetch).catch(() => []),
			searchFacets(base, query, fetch).catch(() => emptyFacets),
		]);
		if (!anyFilter) {
			return { ...filters, home: base, games, loaders, facets, results: [], error: null };
		}
		const results = await searchProjects(base, query, fetch);
		return { ...filters, home: base, games, loaders, facets, results, error: null };
	} catch (cause) {
		return {
			...filters,
			games: [],
			loaders: [],
			facets: emptyFacets,
			results: [],
			error: cause instanceof Error ? cause.message : 'the search failed',
		};
	}
};
