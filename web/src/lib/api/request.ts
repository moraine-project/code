import { normalizeHome } from '../home';

export type Fetcher = typeof fetch;

export class ApiError extends Error {
	readonly status: number;

	constructor(status: number, message: string) {
		super(message);
		this.name = 'ApiError';
		this.status = status;
	}
}

export async function responseError(response: Response, fallback?: string): Promise<ApiError> {
	const text = await response.text();
	return new ApiError(response.status, text || fallback || `request failed (${response.status})`);
}

export function registryUrl(base: string, path: string): string {
	return `${normalizeHome(base)}${path}`;
}
