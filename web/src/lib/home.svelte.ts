import { page } from '$app/state';

import { configuredHome, homeFromUrl, homeLink, isForeignHome } from './home';

const active = $derived(homeFromUrl(page.url));

export const home = {
	get base(): string {
		return active;
	},
	get configured(): string {
		return configuredHome();
	},
	get isForeign(): boolean {
		return isForeignHome(active);
	},
	url(path: string): string {
		return homeLink(path, active, isForeignHome(active));
	},
};
