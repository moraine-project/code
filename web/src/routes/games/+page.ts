import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';
import { listDefinitions, normalizeBase, type DefinitionSummary } from '$lib/api/registry';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	const home = PUBLIC_MORAINE_REGISTRY ?? '';
	try {
		const base = normalizeBase(home);
		const [games, loaders] = await Promise.all([
			listDefinitions(base, 'games', fetch),
			listDefinitions(base, 'loaders', fetch)
		]);
		return { home: base, games, loaders, error: null };
	} catch (cause) {
		return {
			home,
			games: [] as DefinitionSummary[],
			loaders: [] as DefinitionSummary[],
			error: cause instanceof Error ? cause.message : 'the definitions could not be read'
		};
	}
};
