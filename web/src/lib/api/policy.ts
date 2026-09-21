import { z } from 'zod';
import { authorizedFetch, failure } from './session';

const policySchema = z.object({
	project_id: z.string(),
	listing_state: z.string(),
	reason_code: z.string().nullable().optional(),
	reason_note: z.string().nullable().optional(),
	updated_at: z.number().nullable().optional(),
});
export type ListingPolicy = z.infer<typeof policySchema>;

const grantSchema = z.object({
	id: z.string(),
	project_id: z.string(),
	principal: z.string(),
	game_id: z.string(),
	release_kinds: z.string(),
	artifact_kinds: z.string(),
	issued_from_review: z.string(),
	policy_version: z.string(),
	issued_at: z.number(),
	expires_at: z.number().nullable().optional(),
	suspended_at: z.number().nullable().optional(),
	revoked_at: z.number().nullable().optional(),
	reason_code: z.string().nullable().optional(),
});
export type PublicationGrant = z.infer<typeof grantSchema>;

export async function policies(): Promise<ListingPolicy[]> {
	const response = await authorizedFetch('/v1/directory/policy');
	if (!response.ok) throw new Error(await failure(response));
	return z.array(policySchema).parse(await response.json());
}

export async function setPolicy(
	projectId: string,
	listingState: string,
	reasonNote: string,
): Promise<ListingPolicy> {
	const response = await authorizedFetch(`/v1/directory/policy/${encodeURIComponent(projectId)}`, {
		method: 'PUT',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ listing_state: listingState, reason_note: reasonNote || null }),
	});
	if (!response.ok) throw new Error(await failure(response));
	return policySchema.parse(await response.json());
}

export async function publicationGrants(projectId: string): Promise<PublicationGrant[]> {
	const response = await authorizedFetch(
		`/v1/projects/${encodeURIComponent(projectId)}/publication-grants`,
	);
	if (!response.ok) throw new Error(await failure(response));
	return z.array(grantSchema).parse(await response.json());
}

export async function revokePublicationGrants(projectId: string): Promise<number> {
	const response = await authorizedFetch(
		`/v1/projects/${encodeURIComponent(projectId)}/publication-grants`,
		{ method: 'DELETE' },
	);
	if (!response.ok) throw new Error(await failure(response));
	return z.object({ revoked: z.number() }).parse(await response.json()).revoked;
}
