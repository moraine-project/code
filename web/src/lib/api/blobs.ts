import { z } from 'zod';

import { digestHex } from './digests';
import { registryUrl } from './request';
import { authorizedFetch } from './session';

export const uploadReceiptSchema = z.object({
	digest: z.string(),
	size: z.number(),
});

export type UploadReceipt = z.infer<typeof uploadReceiptSchema>;

export function blobUrl(base: string, digest: string): string {
	return registryUrl(base, `/v1/blobs/sha256/${digestHex(digest)}`);
}

export async function fileSha256(file: File): Promise<string> {
	// PERF: WebCrypto has no streaming digest, so large files buffer in memory; the CLI streams.
	const digest = await crypto.subtle.digest('SHA-256', await file.arrayBuffer());
	return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('');
}

export async function uploadBlob(file: File): Promise<UploadReceipt> {
	const response = await authorizedFetch('/v1/blobs', {
		method: 'POST',
		headers: { 'content-type': 'application/octet-stream' },
		body: file,
	});
	if (!response.ok) {
		throw new Error(`upload failed (${response.status})`);
	}
	return uploadReceiptSchema.parse(await response.json());
}
