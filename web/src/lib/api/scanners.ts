import { authorizedFetch, failure } from './session';

async function request<T>(path: string, init?: RequestInit): Promise<T> {
	const response = await authorizedFetch(path, init);
	if (!response.ok) throw await failure(response);
	return (await response.json()) as T;
}

export type ScannerProvider = {
	provider_id: string;
	kind: string;
	command: string;
	args: string[];
	public_key: string;
	enabled: boolean;
};
export type ScanJob = {
	id: string;
	provider_id: string;
	artifact_digest: string;
	status: string;
	requested_by: string;
	attempts: number;
	result?: Record<string, unknown> | null;
	error?: string | null;
};

export const listScannerProviders = () => request<ScannerProvider[]>('/v1/scanners');
export const listScanJobs = () => request<ScanJob[]>('/v1/scans');
export const listScannerPolicies = () =>
	request<Array<{ id: string; provider_id: string; enabled: boolean; auto_scan: boolean }>>(
		'/v1/scanner-policies',
	);
export const listScannerSubscriptions = () =>
	request<Array<Record<string, unknown>>>('/v1/scanner-subscriptions');
export function registerScannerProvider(
	body: Omit<ScannerProvider, 'enabled'> & { enabled?: boolean },
) {
	return request<ScannerProvider>('/v1/scanners', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify(body),
	});
}
export function requestScan(artifact_digest: string, provider_id?: string) {
	return request<{ id: string }>('/v1/scans', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ artifact_digest, provider_id }),
	});
}
export function rescan(id: string) {
	return request<{ id: string }>(`/v1/scans/${encodeURIComponent(id)}/rescan`, { method: 'POST' });
}
export function saveScannerPolicy(body: {
	id: string;
	provider_id: string;
	enabled: boolean;
	auto_scan: boolean;
}) {
	return request<Record<string, unknown>>('/v1/scanner-policies', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify(body),
	});
}
export function createScannerSubscription(
	provider_id: string,
	endpoint: string,
	interval_seconds = 3600,
) {
	return request<{ id: string }>('/v1/scanner-subscriptions', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ provider_id, endpoint, interval_seconds }),
	});
}
