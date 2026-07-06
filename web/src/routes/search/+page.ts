import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';
import { normalizeBase, searchProjects } from '$lib/api/registry';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ url, fetch }) => {
	const home = url.searchParams.get('home') ?? PUBLIC_MORAINE_REGISTRY ?? '';
	const q = url.searchParams.get('q') ?? '';
	if (q.trim().length === 0) {
		return { home, q, results: [], error: null };
	}
	try {
		const base = normalizeBase(home);
		const results = await searchProjects(base, { q, limit: 20 }, fetch);
		return { home: base, q, results, error: null };
	} catch (cause) {
		return { home, q, results: [], error: cause instanceof Error ? cause.message : 'the search failed' };
	}
};
