import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';
import { normalizeBase, searchProjects } from '$lib/api/registry';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ url, fetch }) => {
	const home = url.searchParams.get('home') ?? PUBLIC_MORAINE_REGISTRY ?? '';
	const q = url.searchParams.get('q') ?? '';
	const game = url.searchParams.get('game') ?? '';
	const loader = url.searchParams.get('loader') ?? '';
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
		gameVersion,
		loaderVersion,
		runtimeVersion,
		channel,
		platform,
	].some((value) => value.trim().length > 0);
	if (!anyFilter) {
		return { ...filters, results: [], error: null };
	}
	try {
		const base = normalizeBase(home);
		const results = await searchProjects(
			base,
			{
				q,
				game,
				loader,
				gameVersion,
				loaderVersion,
				runtimeVersion,
				channel,
				platform,
				sort,
				limit: 20,
			},
			fetch,
		);
		return { ...filters, home: base, results, error: null };
	} catch (cause) {
		return {
			...filters,
			results: [],
			error: cause instanceof Error ? cause.message : 'the search failed',
		};
	}
};
