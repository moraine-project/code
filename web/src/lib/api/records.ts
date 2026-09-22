import { authorizedFetch, failure } from './session';
import { hexToBytes } from '$lib/signer';

async function publishWire(path: string, wireHex: string): Promise<Record<string, unknown>> {
	const clean = wireHex.trim().replace(/\s+/g, '');
	if (!clean || clean.length % 2 !== 0 || !/^[0-9a-f]+$/i.test(clean)) {
		throw new Error('signed wire data must be an even-length hexadecimal value');
	}
	const response = await authorizedFetch(path, {
		method: 'POST',
		headers: { 'content-type': 'application/octet-stream' },
		body: hexToBytes(clean) as BodyInit,
	});
	if (!response.ok) throw await failure(response);
	return (await response.json()) as Record<string, unknown>;
}

export function publishAdvisory(wireHex: string) {
	return publishWire('/v1/advisories', wireHex);
}

export function publishAttestation(wireHex: string) {
	return publishWire('/v1/attestations', wireHex);
}

export function publishDenyList(wireHex: string) {
	return publishWire('/v1/deny-lists', wireHex);
}

export function publishMirrorCommitment(wireHex: string) {
	return publishWire('/v1/mirror-commitments', wireHex);
}
