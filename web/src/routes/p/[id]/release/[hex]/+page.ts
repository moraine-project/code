import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';
import { fetchChangelog, fetchProject, fetchRelease, normalizeBase } from '$lib/api/registry';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, url, fetch }) => {
	const home = url.searchParams.get('home') ?? PUBLIC_MORAINE_REGISTRY ?? '';
	try {
		const base = normalizeBase(home);
		const [summary, release] = await Promise.all([
			fetchProject(base, params.id, fetch),
			fetchRelease(base, params.id, params.hex, fetch),
		]);
		const changelog = release?.changelog
			? await fetchChangelog(base, params.id, release.changelog, fetch).catch(() => null)
			: null;
		return {
			home: base,
			projectId: params.id,
			hex: params.hex,
			summary,
			release,
			changelog,
			error: release === null ? 'No such release on this home.' : null,
		};
	} catch (cause) {
		return {
			home,
			projectId: params.id,
			hex: params.hex,
			summary: null,
			release: null,
			changelog: null,
			error: cause instanceof Error ? cause.message : 'the release could not be loaded',
		};
	}
};
