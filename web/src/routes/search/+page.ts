import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';
import { normalizeBase, searchProjects } from '$lib/api/registry';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ url, fetch }) => {
	const home = url.searchParams.get('home') ?? PUBLIC_MORAINE_REGISTRY ?? '';
	const q = url.searchParams.get('q') ?? '';
	const game = url.searchParams.get('game') ?? '';
	if (q.trim().length === 0 && game.length === 0) {
		return { home, q, game, results: [], error: null };
	}
	try {
		const base = normalizeBase(home);
		const results = await searchProjects(base, { q, game, limit: 20 }, fetch);
		return { home: base, q, game, results, error: null };
	} catch (cause) {
		return {
			home,
			q,
			game,
			results: [],
			error: cause instanceof Error ? cause.message : 'the search failed'
		};
	}
};
