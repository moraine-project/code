import { listDefinitions, type DefinitionSummary } from '$lib/api/definitions';
import { homeFromUrl } from '$lib/home';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ url, fetch }) => {
	try {
		const base = homeFromUrl(url);
		const [games, loaders, runtimes] = await Promise.all([
			listDefinitions(base, 'games', fetch),
			listDefinitions(base, 'loaders', fetch),
			listDefinitions(base, 'runtimes', fetch),
		]);
		return { home: base, games, loaders, runtimes, error: null };
	} catch (cause) {
		return {
			home: url.searchParams.get('home') ?? '',
			games: [] as DefinitionSummary[],
			loaders: [] as DefinitionSummary[],
			runtimes: [] as DefinitionSummary[],
			error: cause instanceof Error ? cause.message : 'the definitions could not be read',
		};
	}
};
