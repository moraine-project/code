import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';
import {
	fetchFeed,
	fetchGamePayload,
	fetchProfile,
	fetchProject,
	listLoaders,
	normalizeBase,
} from '$lib/api/registry';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, url, fetch }) => {
	const gameVersion = url.searchParams.get('game_version') ?? undefined;
	const loader = url.searchParams.get('loader') ?? undefined;
	const loaderVersion = url.searchParams.get('loader_version') ?? undefined;
	const requested = url.searchParams.get('home') ?? PUBLIC_MORAINE_REGISTRY ?? '';
	const empty = {
		home: requested,
		projectId: params.id,
		gameVersion: gameVersion ?? '',
		loader: loader ?? '',
		loaderVersion: loaderVersion ?? '',
		summary: null,
		profile: null,
		feed: null,
		gameId: '',
		gamePayload: null,
		loaders: [],
		error: null,
	};

	let base: string;
	try {
		base = normalizeBase(requested);
	} catch (cause) {
		return { ...empty, error: cause instanceof Error ? cause.message : 'invalid home URL' };
	}

	try {
		const [summary, profile] = await Promise.all([
			fetchProject(base, params.id, fetch),
			fetchProfile(base, params.id, fetch).catch(() => null),
		]);
		const from = Math.max(0, summary.head_seq - 50);
		const feed = await fetchFeed(
			base,
			params.id,
			from,
			50,
			{ gameVersion, loader, loaderVersion },
			fetch,
		);
		const newest = feed.entries.toReversed().find((entry) => entry.release);
		const gameId = newest?.release?.game_id ?? '';
		const [gamePayload, loaders] = await Promise.all([
			gameId ? fetchGamePayload(base, gameId, fetch).catch(() => null) : Promise.resolve(null),
			gameId ? listLoaders(base, gameId, fetch).catch(() => []) : Promise.resolve([]),
		]);
		return {
			...empty,
			home: base,
			summary,
			profile,
			feed,
			gameId,
			gamePayload,
			loaders,
		};
	} catch (cause) {
		return {
			...empty,
			home: base,
			error: cause instanceof Error ? cause.message : 'the home could not be reached',
		};
	}
};
