<script lang="ts">
	import { onMount } from 'svelte';
	import {
		policies,
		publicationGrants,
		revokePublicationGrants,
		setPolicy,
		type ListingPolicy,
		type PublicationGrant,
	} from '$lib/api/policy';

	let items = $state<ListingPolicy[]>([]);
	let error = $state<string | null>(null);
	let busy = $state<string | null>(null);
	let policyProject = $state('');
	let policyState = $state('quarantined');
	let policyNote = $state('');
	let grantProject = $state('');
	let grants = $state<PublicationGrant[]>([]);

	onMount(async () => {
		try {
			items = await policies();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not load policy';
		}
	});

	async function save(item: ListingPolicy, event: SubmitEvent) {
		event.preventDefault();
		const form = new FormData(event.currentTarget as HTMLFormElement);
		busy = item.project_id;
		error = null;
		try {
			const updated = await setPolicy(
				item.project_id,
				String(form.get('state')),
				String(form.get('note') ?? ''),
			);
			items =
				updated.listing_state === 'listed'
					? items.filter((entry) => entry.project_id !== updated.project_id)
					: items.map((entry) => (entry.project_id === updated.project_id ? updated : entry));
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not update policy';
		} finally {
			busy = null;
		}
	}

	async function setProjectPolicy(event: SubmitEvent) {
		event.preventDefault();
		busy = 'new-policy';
		error = null;
		try {
			const updated = await setPolicy(policyProject.trim(), policyState, policyNote.trim());
			items =
				updated.listing_state === 'listed'
					? items.filter((item) => item.project_id !== updated.project_id)
					: [updated, ...items.filter((item) => item.project_id !== updated.project_id)];
			policyProject = '';
			policyNote = '';
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not set policy';
		} finally {
			busy = null;
		}
	}

	async function loadGrants() {
		error = null;
		try {
			grants = await publicationGrants(grantProject.trim());
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not load publication grants';
		}
	}

	async function revokeGrants() {
		busy = 'grants';
		error = null;
		try {
			await revokePublicationGrants(grantProject.trim());
			await loadGrants();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not revoke publication grants';
		} finally {
			busy = null;
		}
	}
</script>

<div class="flex flex-col gap-4">
	<p class="text-sm text-base-content/70">
		Listing policy changes this directory’s visibility. It does not rewrite a project’s signed
		history.
	</p>
	{#if error}<div role="alert" class="alert alert-error"><span>{error}</span></div>{/if}
	{#if items.length === 0}<p class="text-sm text-base-content/60">
			No local policy overrides.
		</p>{:else}
		<ul class="flex flex-col gap-3">
			{#each items as item (item.project_id)}
				<li class="rounded-box border border-base-300 p-3">
					<p class="font-mono text-sm">{item.project_id}</p>
					<form class="mt-2 flex flex-wrap items-end gap-2" onsubmit={(event) => save(item, event)}>
						<label class="flex flex-col gap-1 text-xs"
							><span>State</span><select
								class="select select-sm"
								name="state"
								value={item.listing_state}
								><option>listed</option><option>unlisted</option><option>quarantined</option><option
									>blocked</option
								><option>withdrawn</option><option>unavailable</option></select
							></label
						>
						<label class="flex min-w-64 flex-1 flex-col gap-1 text-xs"
							><span>Note</span><input
								class="input input-sm"
								name="note"
								value={item.reason_note ?? ''}
							/></label
						>
						<button class="btn btn-sm" type="submit" disabled={busy === item.project_id}
							>Save</button
						>
					</form>
				</li>
			{/each}
		</ul>
	{/if}
	<form class="flex flex-wrap items-end gap-2" onsubmit={setProjectPolicy}>
		<label class="flex min-w-72 flex-1 flex-col gap-1 text-xs">
			<span>Project id</span>
			<input
				class="input input-sm"
				bind:value={policyProject}
				required
				placeholder="gd:sha256:…"
				aria-label="Policy project id"
			/>
		</label>
		<label class="flex flex-col gap-1 text-xs">
			<span>State</span>
			<select class="select select-sm" bind:value={policyState} aria-label="Policy state">
				<option>unlisted</option><option>quarantined</option><option>blocked</option>
				<option>withdrawn</option><option>unavailable</option><option>listed</option>
			</select>
		</label>
		<label class="flex min-w-64 flex-1 flex-col gap-1 text-xs">
			<span>Note</span>
			<input class="input input-sm" bind:value={policyNote} aria-label="Policy note" />
		</label>
		<button class="btn btn-sm" type="submit" disabled={busy === 'new-policy'}>Set policy</button>
	</form>
	<div class="divider"></div>
	<section class="flex flex-col gap-3">
		<div>
			<h2 class="font-semibold">Progressive publication grants</h2>
			<p class="text-sm text-base-content/70">
				Inspect or revoke the scoped automatic-publication grants issued after review.
			</p>
		</div>
		<form
			class="flex flex-wrap gap-2"
			onsubmit={(event) => {
				event.preventDefault();
				void loadGrants();
			}}
		>
			<input
				class="input input-sm min-w-80"
				bind:value={grantProject}
				required
				placeholder="gd:sha256:…"
				aria-label="Grant project id"
			/>
			<button class="btn btn-sm" type="submit">Load grants</button>
		</form>
		{#if grants.length > 0}
			<ul class="flex flex-col gap-2 text-sm">
				{#each grants as grant (grant.id)}
					<li class="rounded-box border border-base-300 p-2">
						<p class="font-mono">{grant.principal} · {grant.game_id}</p>
						<p class="text-xs text-base-content/60">
							{grant.release_kinds} · {grant.issued_from_review} · {grant.revoked_at
								? 'revoked'
								: grant.suspended_at
									? 'suspended'
									: 'active'}
						</p>
					</li>
				{/each}
			</ul>
			<button
				class="btn btn-sm btn-outline w-fit"
				type="button"
				onclick={revokeGrants}
				disabled={busy === 'grants'}>Revoke active grants</button
			>
		{:else if grantProject}
			<p class="text-sm text-base-content/60">No grants for this project.</p>
		{/if}
	</section>
</div>
