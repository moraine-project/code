import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';
import { fetchPack, normalizeBase } from '$lib/api/registry';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, url, fetch }) => {
	const home = normalizeBase(url.searchParams.get('home') ?? PUBLIC_MORAINE_REGISTRY ?? '');
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
