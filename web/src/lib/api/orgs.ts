import { z } from 'zod';

import { authorizedFetch, failure } from './session';

export const orgMembershipSchema = z.object({
	id: z.string(),
	handle: z.string(),
	display_name: z.string(),
	role: z.string(),
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
		body: JSON.stringify({ handle, display_name: displayName }),
	});
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	return z.object({ id: z.string(), handle: z.string() }).parse(await response.json());
}

export const orgTeamSchema = z.object({
	id: z.string(),
	parent_team_id: z.string().nullable().optional(),
	display_name: z.string(),
});

export const orgDetailSchema = z.object({
	id: z.string(),
	handle: z.string(),
	display_name: z.string(),
	created_at: z.number(),
	teams: z.array(orgTeamSchema),
	projects: z.array(z.string()),
});

export const orgMemberSchema = z.object({
	user_id: z.string(),
	email: z.string(),
	role: z.string(),
	added_at: z.number(),
});

export type OrgTeam = z.infer<typeof orgTeamSchema>;
export type OrgDetail = z.infer<typeof orgDetailSchema>;
export type OrgMember = z.infer<typeof orgMemberSchema>;

export async function orgDetail(handle: string): Promise<OrgDetail> {
	const response = await authorizedFetch(`/v1/orgs/${encodeURIComponent(handle)}`);
	if (response.status === 403) {
		throw new Error('you are not a member of this organization');
	}
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	return orgDetailSchema.parse(await response.json());
}

export async function orgMembers(handle: string): Promise<OrgMember[]> {
	const response = await authorizedFetch(`/v1/orgs/${encodeURIComponent(handle)}/members`);
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	return z.array(orgMemberSchema).parse(await response.json());
}

export async function addOrgMember(handle: string, email: string, role: string): Promise<void> {
	const response = await authorizedFetch(`/v1/orgs/${encodeURIComponent(handle)}/members`, {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ email, role }),
	});
	if (!response.ok) {
		throw new Error(await failure(response));
	}
}

export async function removeOrgMember(handle: string, userId: string): Promise<void> {
	const response = await authorizedFetch(
		`/v1/orgs/${encodeURIComponent(handle)}/members/${encodeURIComponent(userId)}`,
		{ method: 'DELETE' },
	);
	if (!response.ok) {
		throw new Error(await failure(response));
	}
}

export async function createOrgTeam(
	handle: string,
	displayName: string,
	parentTeamId: string | null = null,
): Promise<OrgTeam> {
	const response = await authorizedFetch(`/v1/orgs/${encodeURIComponent(handle)}/teams`, {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ display_name: displayName, parent_team_id: parentTeamId }),
	});
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	return orgTeamSchema.parse(await response.json());
}

export async function reparentOrgTeam(
	handle: string,
	teamId: string,
	parentTeamId: string | null,
): Promise<OrgTeam> {
	const response = await authorizedFetch(
		`/v1/orgs/${encodeURIComponent(handle)}/teams/${encodeURIComponent(teamId)}`,
		{
			method: 'PATCH',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify({ parent_team_id: parentTeamId }),
		},
	);
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	return orgTeamSchema.parse(await response.json());
}
