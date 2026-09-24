import { follows } from '$lib/api/notifications';
import { fetchProfile } from '$lib/api/projects';
import { configuredHome, isForeignHome } from '$lib/home';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	const base = configuredHome();
	const foreign = isForeignHome(base);
	let ids: string[] = [];
	let error: string | null = null;
	try {
		ids = await follows();
	} catch (cause) {
		error = cause instanceof Error ? cause.message : 'could not load your followed projects';
	}
	const projects = await Promise.all(
		ids.map(async (id) => {
			try {
				const profile = await fetchProfile(base, id, fetch);
				return { id, name: profile?.display_name ?? null, summary: profile?.summary ?? null };
			} catch {
				return { id, name: null, summary: null };
			}
		}),
	);
	return { base, projects, error, foreign };
};
