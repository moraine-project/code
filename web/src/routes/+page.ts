import { listDefinitions } from '$lib/api/definitions';
import { searchProjects } from '$lib/api/search';
import { homeFromUrl } from '$lib/home';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ url, fetch }) => {
	try {
		const base = homeFromUrl(url);
		const [games, recent, popular] = await Promise.all([
			listDefinitions(base, 'games', fetch).catch(() => []),
			searchProjects(base, { sort: 'updated', limit: 6 }, fetch)
				.then((page) => page.results)
				.catch(() => []),
			searchProjects(base, { sort: 'popularity', limit: 6 }, fetch)
				.then((page) => page.results)
				.catch(() => []),
		]);
		return { home: base, games, recent, popular, error: null };
	} catch (cause) {
		return {
			home: url.searchParams.get('home') ?? '',
			games: [],
			recent: [],
			popular: [],
			error: cause instanceof Error ? cause.message : 'the home could not be reached',
		};
	}
};
