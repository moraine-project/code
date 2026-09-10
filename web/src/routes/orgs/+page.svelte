<script lang="ts">
	import { onMount } from 'svelte';
	import { Plus, Users } from '@lucide/svelte';
	import { createOrg, myOrgs, type OrgMembership } from '$lib/api/orgs';
	import Avatar from '$lib/components/Avatar.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import PageHeader from '$lib/components/PageHeader.svelte';

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
	<PageHeader
		title="Organizations"
		subtitle="A group that owns projects and teams. Membership is a role on this instance, not a shared login."
	/>

	{#if error}
		<div role="alert" class="alert alert-error"><span>{error}</span></div>
	{/if}

	{#if loading}
		<div class="skeleton h-16 w-full"></div>
	{:else if orgs.length === 0}
		<EmptyState
			title="You are not in an organization yet"
			message="Create one below to own projects as a team, or ask an owner to add you."
		/>
	{:else}
		<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
			{#each orgs as org (org.id)}
				<a
					class="card card-border bg-base-200 transition hover:border-primary"
					href={`/orgs/${encodeURIComponent(org.handle)}`}
				>
					<div class="card-body flex-row items-center gap-4 p-4">
						<Avatar name={org.display_name} id={org.id} size={44} />
						<div class="min-w-0 flex-1">
							<p class="truncate font-semibold">{org.display_name}</p>
							<p class="font-mono text-xs text-base-content/60">{org.handle}</p>
						</div>
						<span class="badge badge-outline badge-sm">{org.role}</span>
					</div>
				</a>
			{/each}
		</div>
	{/if}

	<section class="card card-border bg-base-200">
		<div class="card-body gap-4">
			<h2 class="card-title">
				<Users size={18} />
				Create an organization
			</h2>
			<form class="flex flex-wrap items-end gap-3" onsubmit={submit}>
				<label class="floating-label">
					<span>Handle</span>
					<input
						class="input w-48"
						bind:value={handle}
						required
						pattern="[a-z0-9-]+"
						placeholder="my-team"
						aria-label="Organization handle"
					/>
				</label>
				<label class="floating-label">
					<span>Display name</span>
					<input
						class="input w-56"
						bind:value={displayName}
						required
						placeholder="My Team"
						aria-label="Organization display name"
					/>
				</label>
				<button class="btn" type="submit" disabled={creating}>
					<Plus size={16} />
					Create
				</button>
			</form>
			<p class="text-xs text-base-content/50">
				Handles use lowercase letters, digits, and hyphens. The creator becomes the owner.
			</p>
		</div>
	</section>
</div>
