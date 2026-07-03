import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';
import { fetchFeed, fetchProject, normalizeBase } from '$lib/api/registry';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, url, fetch }) => {
	const requested = url.searchParams.get('home') ?? PUBLIC_MORAINE_REGISTRY ?? '';
	let base: string;
	try {
		base = normalizeBase(requested);
	} catch (cause) {
		return {
			home: requested,
			projectId: params.id,
			summary: null,
			feed: null,
			error: cause instanceof Error ? cause.message : 'invalid home URL'
		};
	}
	try {
		const [summary, feed] = await Promise.all([
			fetchProject(base, params.id, fetch),
			fetchFeed(base, params.id, 0, 50, fetch)
		]);
		return { home: base, projectId: params.id, summary, feed, error: null };
	} catch (cause) {
		return {
			home: base,
			projectId: params.id,
			summary: null,
			feed: null,
			error: cause instanceof Error ? cause.message : 'the home could not be reached'
		};
	}
};
