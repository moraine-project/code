import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';
import { listDefinitions, normalizeBase, searchProjects } from '$lib/api/registry';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ url, fetch }) => {
	const home = url.searchParams.get('home') ?? PUBLIC_MORAINE_REGISTRY ?? '';
	try {
		const base = normalizeBase(home);
		const [games, recent, popular] = await Promise.all([
			listDefinitions(base, 'games', fetch).catch(() => []),
			searchProjects(base, { sort: 'updated', limit: 6 }, fetch).catch(() => []),
			searchProjects(base, { sort: 'popularity', limit: 6 }, fetch).catch(() => []),
		]);
		return { home: base, games, recent, popular, error: null };
	} catch (cause) {
		return {
			home,
			games: [],
			recent: [],
			popular: [],
			error: cause instanceof Error ? cause.message : 'the home could not be reached',
		};
	}
};
