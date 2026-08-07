import { z } from 'zod';

import { authorizedFetch, failure } from './session';

export const submissionSchema = z.object({
	id: z.string(),
	project_id: z.string(),
	object: z.string(),
	entry: z.string(),
	state: z.string(),
	assigned_to: z.string().nullable().optional(),
	submitted_by: z.string(),
	created_at: z.number(),
	updated_at: z.number(),
});

export type Submission = z.infer<typeof submissionSchema>;

export const reasonCodes = [
	'policy-disallowed',
	'malware-suspected',
	'malware-confirmed',
	'impersonation',
	'trademark-claim',
	'rights-complaint',
	'spam',
	'fork-detected',
	'broken',
	'author-request',
] as const;

export async function reviewQueue(): Promise<Submission[]> {
	const response = await authorizedFetch('/v1/review-queue');
	if (response.status === 401 || response.status === 403) {
		throw new Error('sign in with an account that can review submissions');
	}
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	return z.array(submissionSchema).parse(await response.json());
}

export async function assign(id: string): Promise<void> {
	const response = await authorizedFetch(`/v1/submissions/${encodeURIComponent(id)}/assign`, {
		method: 'POST',
	});
	if (!response.ok) {
		throw new Error(await failure(response));
	}
}

export async function decide(
	id: string,
	decision: 'accept' | 'reject' | 'quarantine',
	reasonCode?: string,
): Promise<void> {
	const response = await authorizedFetch(`/v1/submissions/${encodeURIComponent(id)}/review`, {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ decision, reason_code: reasonCode ?? null }),
	});
	if (!response.ok) {
		throw new Error(await failure(response));
	}
}

export const decisionSchema = z.object({
	decision: z.string(),
	reason_code: z.string().nullable().optional(),
	reason_taxonomy_version: z.number(),
	reviewer_id: z.string(),
	decided_at: z.number(),
	appeal_route: z.string().nullable().optional(),
});

export const submissionDetailSchema = z.object({
	submission: submissionSchema,
	decisions: z.array(decisionSchema),
});

export type Decision = z.infer<typeof decisionSchema>;
export type SubmissionDetail = z.infer<typeof submissionDetailSchema>;

export async function mySubmissions(limit = 50, cursor?: string): Promise<SubmissionDetail[]> {
	const params = new URLSearchParams({ limit: String(limit) });
	if (cursor) {
		params.set('cursor', cursor);
	}
	const response = await authorizedFetch(`/v1/submissions?${params.toString()}`);
	if (response.status === 401) {
		throw new Error('sign in to see your submissions');
	}
	if (!response.ok) {
		throw new Error(await failure(response));
	}
	return z.array(submissionDetailSchema).parse(await response.json());
}
