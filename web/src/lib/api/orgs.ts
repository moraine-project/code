import { z } from 'zod';

import { authorizedFetch, failure } from './session';

export const orgMembershipSchema = z.object({
	id: z.string(),
	handle: z.string(),
	display_name: z.string(),
	role: z.string()
});

export type OrgMembership = z.infer<typeof orgMembershipSchema>;

export async function myOrgs(): Promise<OrgMembership[]> {
	const response = await authorizedFetch('/v1/orgs');
	if (response.status === 401) {
		throw new Error('sign in to see your organizations');
	}
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	return z.array(orgMembershipSchema).parse(await response.json());
}

export async function createOrg(handle: string, displayName: string): Promise<{ handle: string }> {
	const response = await authorizedFetch('/v1/orgs', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ handle, display_name: displayName })
	});
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	return z.object({ id: z.string(), handle: z.string() }).parse(await response.json());
}
