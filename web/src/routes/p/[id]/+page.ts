import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';
import { fetchFeed, fetchProfile, fetchProject, normalizeBase } from '$lib/api/registry';
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
			profile: null,
			feed: null,
			error: cause instanceof Error ? cause.message : 'invalid home URL'
		};
	}
	try {
		const [summary, feed, profile] = await Promise.all([
			fetchProject(base, params.id, fetch),
			fetchFeed(base, params.id, 0, 50, fetch),
			fetchProfile(base, params.id, fetch).catch(() => null)
		]);
		return { home: base, projectId: params.id, summary, profile, feed, error: null };
	} catch (cause) {
		return {
			home: base,
			projectId: params.id,
			summary: null,
			profile: null,
			feed: null,
			error: cause instanceof Error ? cause.message : 'the home could not be reached'
		};
	}
};
