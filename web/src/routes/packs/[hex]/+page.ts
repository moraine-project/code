import { fetchPack } from '$lib/api/releases';
import { homeFromUrl } from '$lib/home';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, url, fetch }) => {
	const home = homeFromUrl(url);
	try {
		return { home, pack: await fetchPack(home, params.hex, fetch), error: null };
	} catch (cause) {
		return {
			home,
			pack: null,
			error: cause instanceof Error ? cause.message : 'the pack could not be reached',
		};
	}
};
