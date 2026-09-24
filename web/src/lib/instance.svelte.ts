import { safeExternalUrl } from '$lib/api/external-url';
import { defaultInstance, fetchInstance, type Instance, type NavLink } from '$lib/api/instance';
import type { Fetcher } from '$lib/api/request';
import { homeLink } from '$lib/home';

const DEFAULT_NAME = 'Moraine';

let data = $state<Instance>(defaultInstance);
let logoObjectUrl = $state<string | null>(null);
let generation = 0;

async function loadLogo(logo: string, base: string, fetchFn: Fetcher): Promise<string | null> {
	const target = logo.startsWith('/') ? homeLink(logo, base, false) : safeExternalUrl(logo);
	if (!target) {
		return null;
	}
	const response = await fetchFn(target);
	if (!response.ok || !response.headers.get('content-type')?.startsWith('image/')) {
		return null;
	}
	return URL.createObjectURL(new Blob([await response.arrayBuffer()]));
}

function replaceLogo(next: string | null): void {
	if (logoObjectUrl) {
		URL.revokeObjectURL(logoObjectUrl);
	}
	logoObjectUrl = next;
}

export const instance = {
	get name(): string {
		return data.branding.name ?? DEFAULT_NAME;
	},
	get logoUrl(): string | null {
		return logoObjectUrl;
	},
	get theme(): Record<string, string> {
		return data.branding.theme;
	},
	get nav(): NavLink[] {
		return data.branding.nav;
	},
	get publishing(): string {
		return data.publishing;
	},
	get registration(): string {
		return data.registration;
	},
	get emailVerification(): boolean {
		return data.emailVerification;
	},
	async load(base: string, fetchFn: Fetcher = fetch): Promise<void> {
		const mine = ++generation;
		let loaded: Instance;
		try {
			loaded = await fetchInstance(base, fetchFn);
		} catch {
			if (mine === generation) {
				data = defaultInstance;
				replaceLogo(null);
			}
			return;
		}
		if (mine !== generation) {
			return;
		}
		data = loaded;
		const logo = loaded.branding.logo
			? await loadLogo(loaded.branding.logo, base, fetchFn).catch(() => null)
			: null;
		if (mine !== generation) {
			return;
		}
		replaceLogo(logo);
	},
};
