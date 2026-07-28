<script lang="ts">
	import { onMount } from 'svelte';
	import { createOrg, myOrgs, type OrgMembership } from '$lib/api/orgs';

	let orgs = $state<OrgMembership[]>([]);
	let error = $state<string | null>(null);
	let loading = $state(true);
	let handle = $state('');
	let displayName = $state('');
	let creating = $state(false);

	onMount(load);

	async function load() {
		loading = true;
		try {
			orgs = await myOrgs();
			error = null;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not load organizations';
		} finally {
			loading = false;
		}
	}

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		creating = true;
		try {
			await createOrg(handle.trim().toLowerCase(), displayName.trim());
			handle = '';
			displayName = '';
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not create the organization';
		} finally {
			creating = false;
		}
	}
</script>

<svelte:head>
	<title>Organizations · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<h1 class="text-2xl font-bold">Organizations</h1>
	<p class="text-base-content/80 max-w-2xl">
		An organization is a group that owns projects and teams. Membership is a role grant on this
		instance, not a shared login, and it never grants signing authority by itself.
	</p>

	{#if error}
		<div role="alert" class="alert alert-error"><span>{error}</span></div>
	{/if}

	{#if loading}
		<span class="loading loading-spinner loading-sm" aria-hidden="true"></span>
		<span class="sr-only" role="status">Loading organizations</span>
	{:else if orgs.length === 0}
		<p class="text-base-content/60 text-sm">You do not belong to an organization yet.</p>
	{:else}
		<ul class="flex flex-col gap-2">
			{#each orgs as org (org.id)}
				<li class="card card-border">
					<div class="card-body flex-row items-center justify-between gap-2 py-3">
						<div>
							<span class="font-semibold">{org.display_name}</span>
							<span class="text-base-content/60 font-mono text-xs">{org.handle}</span>
						</div>
						<span class="badge badge-outline">{org.role}</span>
					</div>
				</li>
			{/each}
		</ul>
	{/if}

	<section class="card card-border">
		<div class="card-body">
			<h2 class="card-title">Create an organization</h2>
			<form class="flex flex-wrap items-end gap-3" onsubmit={submit}>
				<label class="form-control">
					<span class="label-text">Handle</span>
					<input
						class="input input-bordered"
						bind:value={handle}
						required
						pattern="[a-z0-9-]+"
						aria-label="Organization handle"
					/>
				</label>
				<label class="form-control">
					<span class="label-text">Display name</span>
					<input class="input input-bordered" bind:value={displayName} required aria-label="Organization display name" />
				</label>
				<button class="btn" type="submit" disabled={creating}>Create</button>
			</form>
			<p class="text-base-content/60 text-sm">
				Handles are lowercase letters, digits, and hyphens. The creator becomes the owner.
			</p>
		</div>
	</section>
</div>
