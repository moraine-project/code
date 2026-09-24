import { z } from 'zod';

import { digestHex } from './digests';
import type { Fetcher } from './request';
import { registryUrl } from './request';

export const artifactSchema = z.object({
	digest: z.string(),
	size: z.number(),
	media_type: z.string(),
	filename: z.string(),
	is_primary: z.boolean(),
});

export const compatibilitySchema = z.object({
	scheme: z.string(),
	values: z.array(z.string()),
	loader_id: z.string().nullable().optional(),
	side: z.string(),
});

export const dependencySchema = z.object({
	target_kind: z.string(),
	target_id: z.string(),
	kind: z.string(),
});

export const rightsSchema = z.object({
	redistribution: z.string(),
	modpack_inclusion: z.string(),
	mirroring: z.string(),
	attribution_required: z.boolean(),
});

export const withdrawalSchema = z.object({
	reason: z.string(),
	note: z.string().nullable().optional(),
	declared_time: z.number(),
});

export const advisorySchema = z.object({
	advisory: z.string(),
	provider_id: z.string(),
	project_id: z.string(),
	severity: z.string(),
	category: z.string(),
	block_promotion: z.boolean(),
	affected_digest: z.string().nullable().optional(),
	published_at: z.number(),
	retracted_at: z.number().nullable().optional(),
});

export const attestationSchema = z.object({
	attestation: z.string(),
	kind: z.string(),
	signer_id: z.string(),
	subject_kind: z.string(),
	subject_id: z.string(),
	media_type: z.string(),
	issued_at: z.number(),
	body_digest: z.string().nullable().optional(),
	has_inline_body: z.boolean(),
});

export const releaseSchema = z.object({
	project_id: z.string(),
	human_version: z.string(),
	channel: z.string(),
	kind: z.string(),
	declared_time: z.number(),
	license_expression: z.string().nullable().optional(),
	artifacts: z.array(artifactSchema),
	compatibility: z.array(compatibilitySchema),
	dependencies: z.array(dependencySchema),
	rights: rightsSchema.nullable().optional(),
	withdrawal: withdrawalSchema.nullable().optional(),
	advisories: z.array(advisorySchema).optional(),
	attestations: z.array(attestationSchema).optional(),
	changelog: z.string().nullable().optional(),
});

export const changelogSchema = z.object({
	project_id: z.string(),
	release: z.string().nullable().optional(),
	locale_sections: z.array(
		z.object({
			locale: z.string(),
			sections: z.array(
				z.object({
					heading: z.string(),
					body: z.string(),
					severity: z.string().nullable().optional(),
				}),
			),
		}),
	),
	declared_time: z.number(),
});

export const packSchema = z.object({
	pack: z.string(),
	payload: z.record(z.string(), z.unknown()),
});

export const lookupMatchSchema = z.object({
	project_id: z.string(),
	release: z.string(),
	human_version: z.string().nullable().optional(),
	filename: z.string().nullable().optional(),
});

export const digestLookupSchema = z.object({
	digest: z.string(),
	matches: z.array(lookupMatchSchema),
});

export const artifactLocationsSchema = z.object({
	protocol: z.number(),
	algorithm: z.literal('sha256'),
	digest: z.string(),
	size: z.number().nullable().optional(),
	locations: z.array(
		z.object({
			url: z.string(),
			kind: z.string(),
			provenance: z.string(),
			operator_id: z.string().nullable().optional(),
			location_record_digest: z.string().nullable().optional(),
			commitment_digest: z.string().nullable().optional(),
			supports_ranges: z.boolean().nullable().optional(),
			last_success_at: z.number().nullable().optional(),
			expires_at: z.number().nullable().optional(),
			priority: z.number().nullable().optional(),
		}),
	),
	refreshed_at: z.number(),
});

export type Pack = z.infer<typeof packSchema>;
export type Release = z.infer<typeof releaseSchema>;
export type Changelog = z.infer<typeof changelogSchema>;
export type Attestation = z.infer<typeof attestationSchema>;
export type Artifact = z.infer<typeof artifactSchema>;
export type DigestLookup = z.infer<typeof digestLookupSchema>;
export type ArtifactLocations = z.infer<typeof artifactLocationsSchema>;

export async function fetchRelease(
	base: string,
	projectId: string,
	hex: string,
	fetchFn: Fetcher = fetch,
): Promise<Release | null> {
	const response = await fetchFn(
		registryUrl(base, `/v1/projects/${encodeURIComponent(projectId)}/releases/${hex}`),
	);
	if (response.status === 404) {
		return null;
	}
	if (!response.ok) {
		throw new Error(`home returned ${response.status} for the release`);
	}
	return releaseSchema.parse(await response.json());
}

export async function fetchChangelog(
	base: string,
	projectId: string,
	digest: string,
	fetchFn: Fetcher = fetch,
): Promise<Changelog | null> {
	const response = await fetchFn(
		registryUrl(
			base,
			`/v1/projects/${encodeURIComponent(projectId)}/changelog/${digestHex(digest)}`,
		),
	);
	if (response.status === 404) {
		return null;
	}
	if (!response.ok) {
		throw new Error(`home returned ${response.status} for the changelog`);
	}
	return changelogSchema.parse(await response.json());
}

export async function fetchPack(
	base: string,
	hex: string,
	fetchFn: Fetcher = fetch,
): Promise<Pack | null> {
	const response = await fetchFn(registryUrl(base, `/v1/packs/${encodeURIComponent(hex)}`));
	if (response.status === 404) return null;
	if (!response.ok) throw new Error(`home returned ${response.status} for the modpack manifest`);
	return packSchema.parse(await response.json());
}

export async function lookupDigest(
	base: string,
	digest: string,
	fetchFn: Fetcher = fetch,
): Promise<DigestLookup> {
	const response = await fetchFn(
		registryUrl(base, `/v1/lookup?sha256=${encodeURIComponent(digest.trim())}`),
	);
	if (!response.ok) {
		throw new Error(`home returned ${response.status} for the digest`);
	}
	return digestLookupSchema.parse(await response.json());
}

export async function mirrorLocations(
	base: string,
	digest: string,
	fetchFn: Fetcher = fetch,
): Promise<ArtifactLocations> {
	const response = await fetchFn(
		registryUrl(base, `/v1/artifacts/sha256/${encodeURIComponent(digestHex(digest))}/locations`),
	);
	if (!response.ok) throw new Error(`home returned ${response.status} for artifact locations`);
	return artifactLocationsSchema.parse(await response.json());
}
