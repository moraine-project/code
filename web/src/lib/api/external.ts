import { z } from 'zod';

import { normalizeBase, type Fetcher } from './registry';

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
