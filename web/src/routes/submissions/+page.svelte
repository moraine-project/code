<script lang="ts">
	import { onMount } from 'svelte';
	import Digest from '$lib/components/Digest.svelte';
	import { mySubmissions, type SubmissionDetail } from '$lib/api/review';

	const pageSize = 50;

	let items = $state<SubmissionDetail[]>([]);
	let error = $state<string | null>(null);
	let loading = $state(true);
	let more = $state(false);
	let hasMore = $state(false);

	onMount(load);

	async function load() {
		loading = true;
		try {
			items = await mySubmissions(pageSize);
			hasMore = items.length === pageSize;
			error = null;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not load your submissions';
		} finally {
			loading = false;
		}
	}

	async function loadMore() {
		more = true;
		try {
			const oldest = items.at(-1)?.submission.created_at;
			const page = await mySubmissions(pageSize, oldest);
			items = items.concat(page);
			hasMore = page.length === pageSize;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not load more submissions';
		} finally {
			more = false;
		}
	}

	function formatTime(seconds: number): string {
		return new Date(seconds * 1000).toLocaleString();
	}
</script>

<svelte:head>
	<title>Your submissions · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<h1 class="text-2xl font-bold">Your submissions</h1>
	<p class="text-base-content/80 max-w-2xl">
		Acceptance is this home's listing decision, not a safety guarantee. A rejection keeps the
		signed record for audit and states a reason; it does not change what your keys authorize, and
		you can publish the same signed release through another home.
	</p>

	{#if error}
		<div role="alert" class="alert alert-error"><span>{error}</span></div>
	{/if}

	{#if loading}
		<span class="loading loading-spinner loading-sm" aria-hidden="true"></span>
		<span class="sr-only" role="status">Loading your submissions</span>
	{:else if items.length === 0}
		<p class="text-base-content/60 text-sm">You have not submitted a release to this home.</p>
	{:else}
		<ul class="flex flex-col gap-3">
			{#each items as item (item.submission.id)}
				<li class="card card-border">
					<div class="card-body">
						<div class="flex flex-wrap items-center justify-between gap-2">
							<div class="flex flex-wrap items-center gap-2">
								<span class="badge badge-outline">{item.submission.state}</span>
								<Digest value={item.submission.object} label="the object id" />
							</div>
							<span class="text-base-content/60 text-sm">{formatTime(item.submission.created_at)}</span>
						</div>
						{#each item.decisions as decision (decision.decided_at)}
							<p class="text-base-content/80 text-sm">
								<strong>Decision:</strong>
								{decision.decision}
								{#if decision.reason_code}
									— {decision.reason_code}
								{/if}
								{#if decision.appeal_route}
									· appeal: <a class="link" href={decision.appeal_route}>{decision.appeal_route}</a>
								{/if}
							</p>
						{/each}
					</div>
				</li>
			{/each}
		</ul>
		{#if hasMore}
			<button class="btn btn-outline w-fit" type="button" onclick={loadMore} disabled={more}>
				Load older submissions
			</button>
		{/if}
	{/if}
</div>
