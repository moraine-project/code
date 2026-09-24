import { DEFINITION_KINDS, fetchDefinition } from '$lib/api/definitions';
import { homeFromUrl } from '$lib/home';
import type { PageLoad } from './$types';

const kinds = DEFINITION_KINDS;

export const load: PageLoad = async ({ url, params, fetch }) => {
	const kind = kinds.find((candidate) => candidate === params.kind);
	if (!kind) {
		return {
			home: url.searchParams.get('home') ?? '',
			kind: params.kind,
			id: params.id,
			definition: null,
			error: 'Unknown definition kind.',
		};
	}
	try {
		const base = homeFromUrl(url);
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
			home: url.searchParams.get('home') ?? '',
			kind,
			id: params.id,
			definition: null,
			error: cause instanceof Error ? cause.message : 'the definition could not be loaded',
		};
	}
};
