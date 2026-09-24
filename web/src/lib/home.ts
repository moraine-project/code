import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';

export function isAllowedHomeHost(hostname: string): boolean {
	const host = hostname.replace(/^\[|\]$/g, '').toLowerCase();
	if (host === 'localhost' || host.endsWith('.localhost')) {
		return true;
	}
	const ipv4 = host.match(/^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/);
	if (ipv4) {
		const [first, second] = ipv4.slice(1).map(Number);
		if (first === 127) return true;
		if (first === 0 || first === 10) return false;
		if (first === 172 && second >= 16 && second <= 31) return false;
		if (first === 192 && second === 168) return false;
		if (first === 169 && second === 254) return false;
		return true;
	}
	if (host.includes(':')) {
		if (host === '::1') return true;
		if (host === '::') return false;
		if (host.startsWith('fc') || host.startsWith('fd') || host.startsWith('fe80')) return false;
		return true;
	}
	return true;
}

export function normalizeHome(value: string | null | undefined): string {
	const trimmed = (value ?? '').trim().replace(/\/+$/, '');
	if (trimmed.length === 0) {
		throw new Error('a home registry URL is required');
	}
	let url: URL;
	try {
		url = new URL(trimmed);
	} catch {
		throw new Error('the home must be an http(s) URL');
	}
	if (url.protocol !== 'http:' && url.protocol !== 'https:') {
		throw new Error('the home must be an http(s) URL');
	}
	if (!isAllowedHomeHost(url.hostname)) {
		throw new Error('the home must be a public host');
	}
	return trimmed;
}

export function configuredHome(): string {
	const configured = (PUBLIC_MORAINE_REGISTRY ?? '').trim();
	return configured.length === 0 ? '' : normalizeHome(configured);
}

export function homeFromUrl(url: URL): string {
	const requested = url.searchParams.get('home');
	if (requested && requested.trim().length > 0) {
		return normalizeHome(requested);
	}
	return configuredHome();
}

export function isForeignHome(base: string): boolean {
	return base !== configuredHome();
}

export function homeLink(path: string, base: string, foreign: boolean): string {
	if (!foreign) {
		return path;
	}
	const [route, query] = path.split('?');
	const params = new URLSearchParams(query);
	params.set('home', base);
	return `${route}?${params.toString()}`;
}
