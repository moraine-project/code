import { normalizeHome } from '../home';

export type Fetcher = typeof fetch;

export function registryUrl(base: string, path: string): string {
	return `${normalizeHome(base)}${path}`;
}
