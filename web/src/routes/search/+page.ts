import { fetchGamePayload, listDefinitions, listLoaders } from '$lib/api/definitions';
import { emptyFacets, searchFacets, searchProjects } from '$lib/api/search';
import { homeFromUrl } from '$lib/home';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ url, fetch }) => {
	const q = url.searchParams.get('q') ?? '';
	const game = url.searchParams.get('game') ?? '';
	const loader = url.searchParams.get('loader') ?? '';
	const category = url.searchParams.get('category') ?? '';
	const tag = url.searchParams.get('tag') ?? '';
	const gameVersion = url.searchParams.get('game_version') ?? '';
	const channel = url.searchParams.get('channel') ?? '';
	const sort = url.searchParams.get('sort') ?? '';
	const query = { q, game, loader, category, tag, gameVersion, channel, sort, limit: 20 };
	const filtered = [q, game, loader, category, tag, gameVersion, channel].some(
		(value) => value.trim().length > 0,
	);

	try {
		const base = homeFromUrl(url);
		const [games, loaders, facets, gamePayload] = await Promise.all([
			listDefinitions(base, 'games', fetch).catch(() => []),
			listLoaders(base, game || undefined, fetch).catch(() => []),
			searchFacets(base, query, fetch).catch(() => emptyFacets),
			game ? fetchGamePayload(base, game, fetch).catch(() => null) : Promise.resolve(null),
		]);
		const results = filtered ? await searchProjects(base, query, fetch) : [];
		return {
			home: base,
			q,
			game,
			loader,
			category,
			tag,
			gameVersion,
			channel,
			sort,
			games,
			loaders,
			facets,
			gamePayload,
			results,
			error: null,
		};
	} catch (cause) {
		return {
			home: url.searchParams.get('home') ?? '',
			q,
			game,
			loader,
			category,
			tag,
			gameVersion,
			channel,
			sort,
			games: [],
			loaders: [],
			facets: emptyFacets,
			gamePayload: null,
			results: [],
			error: cause instanceof Error ? cause.message : 'the search failed',
		};
	}
};
