import { z } from 'zod';

import { normalizeBase, type Fetcher } from './registry';
import { authorizedFetch, failure } from './session';

export const externalProjectSchema = z.object({
	provider: z.string(),
	external_project_id: z.string(),
	source_class: z.string(),
	canonical_source_url: z.string(),
	observed_profile: z.record(z.string(), z.unknown()),
	files: z.array(
		z.object({
			external_file_id: z.string(),
			source_url: z.string(),
			digest: z.string().nullable().optional(),
			size: z.number().nullable().optional(),
			metadata: z.record(z.string(), z.unknown()),
			observed_at: z.number(),
			deleted_at: z.number().nullable().optional(),
		}),
	),
	observed_at: z.number(),
	last_synced_at: z.number(),
	source_state: z.string(),
	bridge_id: z.string(),
	bridge_version: z.string(),
	linked_native_project_id: z.string().nullable().optional(),
});
export type ExternalProject = z.infer<typeof externalProjectSchema>;

const claimSchema = z.object({
	id: z.string(),
	provider: z.string(),
	external_project_id: z.string(),
	claimant_ref: z.string(),
	challenge_ref: z.string(),
	state: z.string(),
	recorded_by: z.string(),
	recorded_at: z.number(),
	expires_at: z.number(),
	verified_at: z.number().nullable().optional(),
	reviewed_by: z.string().nullable().optional(),
});
export type ExternalClaim = z.infer<typeof claimSchema>;

export async function createExternalClaim(
	provider: string,
	externalProjectId: string,
	claimantRef: string,
	challengeRef: string,
): Promise<{ id: string; state: string; challenge_ref: string; expires_at: number }> {
	const response = await authorizedFetch(
		`/v1/external-projects/${encodeURIComponent(provider)}/${encodeURIComponent(externalProjectId)}/claim`,
		{
			method: 'POST',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify({ claimant_ref: claimantRef, challenge_ref: challengeRef }),
		},
	);
	if (!response.ok) throw new Error(await failure(response));
	return z
		.object({
			id: z.string(),
			state: z.string(),
			challenge_ref: z.string(),
			expires_at: z.number(),
		})
		.parse(await response.json());
}

export async function externalClaims(state = 'pending'): Promise<ExternalClaim[]> {
	const response = await authorizedFetch(
		`/v1/external-project-claims?state=${encodeURIComponent(state)}`,
	);
	if (!response.ok) throw new Error(await failure(response));
	return z.array(claimSchema).parse(await response.json());
}

export async function reviewExternalClaim(
	id: string,
	state: 'approved' | 'rejected',
): Promise<void> {
	const response = await authorizedFetch(
		`/v1/external-project-claims/${encodeURIComponent(id)}/review`,
		{
			method: 'POST',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify({ state }),
		},
	);
	if (!response.ok) throw new Error(await failure(response));
}

export async function fetchExternalProject(
	base: string,
	provider: string,
	externalProjectId: string,
	fetchFn: Fetcher = fetch,
): Promise<ExternalProject> {
	const response = await fetchFn(
		`${normalizeBase(base)}/v1/external-projects/${encodeURIComponent(provider)}/${encodeURIComponent(externalProjectId)}`,
	);
	if (!response.ok) {
		throw new Error(`home returned ${response.status} for the external project`);
	}
	return externalProjectSchema.parse(await response.json());
}
