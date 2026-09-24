import { fetchExternalProject } from '$lib/api/external';
import { homeFromUrl } from '$lib/home';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, url, fetch }) => {
	const requested = url.searchParams.get('home') ?? '';
	try {
		const home = homeFromUrl(url);
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
