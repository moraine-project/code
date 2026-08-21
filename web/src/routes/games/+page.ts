import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';
import { listDefinitions, normalizeBase, type DefinitionSummary } from '$lib/api/registry';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	const home = PUBLIC_MORAINE_REGISTRY ?? '';
	try {
		const base = normalizeBase(home);
		const [games, loaders, runtimes] = await Promise.all([
			listDefinitions(base, 'games', fetch),
			listDefinitions(base, 'loaders', fetch),
			listDefinitions(base, 'runtimes', fetch),
		]);
		return { home: base, games, loaders, runtimes, error: null };
	} catch (cause) {
		return {
			home,
			games: [] as DefinitionSummary[],
			loaders: [] as DefinitionSummary[],
			runtimes: [] as DefinitionSummary[],
			error: cause instanceof Error ? cause.message : 'the definitions could not be read',
		};
	}
};
