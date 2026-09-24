import { fetchProfile, fetchProject } from '$lib/api/projects';
import { fetchChangelog, fetchRelease } from '$lib/api/releases';
import { homeFromUrl } from '$lib/home';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, url, fetch }) => {
	try {
		const base = homeFromUrl(url);
		const [summary, release, profile] = await Promise.all([
			fetchProject(base, params.id, fetch),
			fetchRelease(base, params.id, params.hex, fetch),
			fetchProfile(base, params.id, fetch).catch(() => null),
		]);
		const changelog = release?.changelog
			? await fetchChangelog(base, params.id, release.changelog, fetch).catch(() => null)
			: null;
		return {
			home: base,
			projectId: params.id,
			hex: params.hex,
			summary,
			profile,
			release,
			changelog,
			error: release === null ? 'No such release on this home.' : null,
		};
	} catch (cause) {
		return {
			home: url.searchParams.get('home') ?? '',
			projectId: params.id,
			hex: params.hex,
			summary: null,
			profile: null,
			release: null,
			changelog: null,
			error: cause instanceof Error ? cause.message : 'the release could not be loaded',
		};
	}
};
