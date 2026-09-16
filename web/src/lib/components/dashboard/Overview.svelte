<script lang="ts">
	import { onMount } from 'svelte';
	import { overview, type Overview } from '$lib/api/admin';

	let stats = $state<Overview | null>(null);
	let error = $state<string | null>(null);

	onMount(async () => {
		try {
			stats = await overview();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not read the instance overview';
		}
	});

	const cards = $derived<[string, number, string | null][]>(
		stats
			? [
					['Projects', stats.projects, '/search?sort=updated'],
					['Definitions', stats.definitions, '/games'],
					['Accounts', stats.accounts, null],
					['Unverified accounts', stats.unverified_accounts, null],
					['Pending review', stats.pending_submissions, '/review'],
					['Followed homes', stats.followed_homes, null],
					['Advisories', stats.advisories, null],
				]
			: [],
	);
</script>

{#if error}
	<div role="alert" class="alert alert-error"><span>{error}</span></div>
{:else if !stats}
	<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
		{#each Array(4) as _, index (index)}
			<div class="skeleton h-24"></div>
		{/each}
	</div>
{:else}
	<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
		{#each cards as [label, value, href] (label)}
			<div class="card card-border bg-base-200">
				<div class="card-body gap-1 p-4">
					<p class="text-sm text-base-content/60">{label}</p>
					<p class="text-2xl font-semibold">{value}</p>
					{#if href}
						<a class="link link-hover text-xs" {href}>View</a>
					{/if}
				</div>
			</div>
		{/each}
	</div>
{/if}
