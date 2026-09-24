<script lang="ts">
	import { onMount } from 'svelte';
	import { fetchExternalProject, upsertExternalProject } from '$lib/api/external';
	import { mirrorLocations } from '$lib/api/releases';
	import {
		publishAdvisory,
		publishAttestation,
		publishDenyList,
		publishMirrorCommitment,
	} from '$lib/api/records';
	import { apiOrigin } from '$lib/api/session';

	type Action = 'advisory' | 'attestation' | 'deny-list' | 'mirror-commitment';
	let wire = $state<Record<Action, string>>({
		advisory: '',
		attestation: '',
		'deny-list': '',
		'mirror-commitment': '',
	});
	let busy = $state<Action | null>(null);
	let error = $state<string | null>(null);
	let notice = $state<string | null>(null);
	let digest = $state('');
	let locations = $state<unknown>(null);
	let provider = $state('');
	let externalId = $state('');
	let externalJson = $state('');
	let externalResult = $state<unknown>(null);

	onMount(() => {
		externalJson = JSON.stringify(
			{
				source_class: 'external-catalog',
				canonical_source_url: 'https://example.org/project',
				observed_profile: { name: 'Example project' },
				files: [],
				observed_at: Math.floor(Date.now() / 1000),
				source_state: 'active',
				bridge_id: 'operator-bridge',
				bridge_version: '1',
			},
			null,
			2,
		);
	});

	async function publish(action: Action) {
		busy = action;
		error = null;
		notice = null;
		try {
			const result = await {
				advisory: publishAdvisory,
				attestation: publishAttestation,
				'deny-list': publishDenyList,
				'mirror-commitment': publishMirrorCommitment,
			}[action](wire[action]);
			notice = `${action} accepted: ${Object.values(result)[0] ?? 'recorded'}`;
			wire[action] = '';
		} catch (cause) {
			error = cause instanceof Error ? cause.message : `could not publish ${action}`;
		} finally {
			busy = null;
		}
	}

	async function findLocations(event: SubmitEvent) {
		event.preventDefault();
		error = null;
		notice = null;
		try {
			locations = await mirrorLocations(apiOrigin(), digest.trim());
			notice = 'Loaded artifact locations and mirror commitments.';
		} catch (cause) {
			locations = null;
			error = cause instanceof Error ? cause.message : 'could not load artifact locations';
		}
	}

	async function saveExternal() {
		error = null;
		notice = null;
		try {
			const request = JSON.parse(externalJson) as Parameters<typeof upsertExternalProject>[2];
			externalResult = await upsertExternalProject(provider.trim(), externalId.trim(), request);
			notice = 'External catalog observation saved.';
		} catch (cause) {
			error =
				cause instanceof Error ? cause.message : 'could not save external catalog observation';
		}
	}

	async function loadExternal() {
		error = null;
		notice = null;
		try {
			externalResult = await fetchExternalProject(apiOrigin(), provider.trim(), externalId.trim());
			notice = 'Loaded external catalog observation.';
		} catch (cause) {
			externalResult = null;
			error =
				cause instanceof Error ? cause.message : 'could not load external catalog observation';
		}
	}

	const records: Array<{ key: Action; title: string; description: string }> = [
		{
			key: 'advisory',
			title: 'Signed advisory',
			description: 'Publish provider evidence for a project or artifact.',
		},
		{
			key: 'attestation',
			title: 'Signed attestation',
			description: 'Publish review, scanner, SBOM, or compatibility evidence.',
		},
		{
			key: 'deny-list',
			title: 'Signed deny list',
			description: 'Replace this issuer’s active policy list.',
		},
		{
			key: 'mirror-commitment',
			title: 'Mirror commitment',
			description: 'Publish proof that a pinned mirror holds an artifact.',
		},
	];
</script>

<div class="flex flex-col gap-6">
	<p class="text-sm text-base-content/70">
		These records remain signed by their provider, issuer, or mirror key. The dashboard transports
		the wire bytes; it cannot turn an account session into signing authority.
	</p>
	{#if error}<div role="alert" class="alert alert-error"><span>{error}</span></div>{/if}
	{#if notice}<div role="alert" class="alert alert-success"><span>{notice}</span></div>{/if}

	<section class="grid gap-4 lg:grid-cols-2">
		{#each records as record}
			<form
				class="flex flex-col gap-3 rounded-box border border-base-300 p-4"
				onsubmit={(event) => {
					event.preventDefault();
					publish(record.key);
				}}
			>
				<div>
					<h2 class="font-semibold">{record.title}</h2>
					<p class="text-sm text-base-content/70">{record.description}</p>
				</div>
				<label class="floating-label">
					<span>Signed wire bytes (hex)</span>
					<textarea
						class="textarea min-h-28 w-full font-mono text-xs"
						aria-label={`${record.title} signed wire`}
						bind:value={wire[record.key]}
						required></textarea>
				</label>
				<button class="btn btn-primary w-fit" type="submit" disabled={busy !== null}
					>Publish {record.title.toLowerCase()}</button
				>
			</form>
		{/each}
	</section>

	<section class="flex flex-col gap-3 rounded-box border border-base-300 p-4">
		<div>
			<h2 class="font-semibold">Artifact locations</h2>
			<p class="text-sm text-base-content/70">
				Inspect ordered origins, external links, and verified mirror commitments for a digest.
			</p>
		</div>
		<form class="flex flex-wrap items-end gap-3" onsubmit={findLocations}>
			<label class="floating-label"
				><span>Artifact digest</span><input
					class="input w-80 font-mono"
					aria-label="Artifact digest"
					bind:value={digest}
					placeholder="sha256:…"
					required
				/></label
			>
			<button class="btn btn-outline" type="submit">Find locations</button>
		</form>
		{#if locations}<pre
				class="max-h-72 overflow-auto rounded-box bg-base-300 p-3 text-xs">{JSON.stringify(
					locations,
					null,
					2,
				)}</pre>{/if}
	</section>

	<section class="flex flex-col gap-3 rounded-box border border-base-300 p-4">
		<div>
			<h2 class="font-semibold">External catalog observation</h2>
			<p class="text-sm text-base-content/70">
				Store a provenance-labelled catalog record or inspect an existing one before opening a
				claim.
			</p>
		</div>
		<div class="grid gap-3 sm:grid-cols-2">
			<label class="floating-label"
				><span>Provider</span><input
					class="input"
					aria-label="External provider"
					bind:value={provider}
					required
				/></label
			>
			<label class="floating-label"
				><span>External project ID</span><input
					class="input"
					aria-label="External project ID"
					bind:value={externalId}
					required
				/></label
			>
		</div>
		<label class="floating-label"
			><span>Observation JSON</span><textarea
				class="textarea min-h-56 w-full font-mono text-xs"
				aria-label="Observation JSON"
				bind:value={externalJson}
				required></textarea></label
		>
		<div class="flex flex-wrap gap-2">
			<button class="btn btn-primary" type="button" onclick={saveExternal}>Save observation</button
			><button class="btn btn-outline" type="button" onclick={loadExternal}>Load observation</button
			>
		</div>
		{#if externalResult}<pre
				class="max-h-72 overflow-auto rounded-box bg-base-300 p-3 text-xs">{JSON.stringify(
					externalResult,
					null,
					2,
				)}</pre>{/if}
	</section>
</div>
