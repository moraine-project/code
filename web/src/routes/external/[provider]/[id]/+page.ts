import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';
import { fetchExternalProject } from '$lib/api/external';
import { normalizeBase } from '$lib/api/registry';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, url, fetch }) => {
	const requested = url.searchParams.get('home') ?? PUBLIC_MORAINE_REGISTRY ?? '';
	try {
		const home = normalizeBase(requested);
		return {
			home,
			project: await fetchExternalProject(home, params.provider, params.id, fetch),
			error: null,
		};
	} catch (cause) {
		return {
			home: requested,
			project: null,
			error: cause instanceof Error ? cause.message : 'the external project could not be reached',
		};
	}
};
