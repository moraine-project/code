import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';
import { fetchDefinition, normalizeBase } from '$lib/api/registry';
import type { PageLoad } from './$types';

const kinds = ['games', 'loaders', 'runtimes'] as const;

export const load: PageLoad = async ({ params, fetch }) => {
	const home = PUBLIC_MORAINE_REGISTRY ?? '';
	const kind = kinds.find((candidate) => candidate === params.kind);
	if (!kind) {
		return {
			home,
			kind: params.kind,
			id: params.id,
			definition: null,
			error: 'Unknown definition kind.',
		};
	}
	try {
		const base = normalizeBase(home);
		const definition = await fetchDefinition(base, kind, params.id, fetch);
		return {
			home: base,
			kind,
			id: params.id,
			definition,
			error: definition === null ? 'No such definition on this home.' : null,
		};
	} catch (cause) {
		return {
			home,
			kind,
			id: params.id,
			definition: null,
			error: cause instanceof Error ? cause.message : 'the definition could not be loaded',
		};
	}
};
