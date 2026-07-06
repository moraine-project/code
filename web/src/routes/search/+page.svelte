<script lang="ts">
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { shortDigest } from '$lib/api/registry';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let query = $state(untrack(() => data.q));

	function submit(event: SubmitEvent) {
		event.preventDefault();
		const params = new URLSearchParams();
		if (query.trim().length > 0) params.set('q', query.trim());
		if (data.home) params.set('home', data.home);
		goto(`/search?${params.toString()}`);
	}

	function homeParam(): string {
		return data.home ? `?home=${encodeURIComponent(data.home)}` : '';
	}
</script>

<svelte:head>
	<title>Search · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<h1 class="text-2xl font-bold">Search</h1>
	<p class="text-base-content/80 max-w-2xl">
		Search runs against this directory's own index of validated records. Ranking is local policy,
		not a safety signal, and an unlisted project can still be reached by its direct URL.
	</p>

	<form class="join w-full max-w-2xl" onsubmit={submit}>
		<input
			class="input join-item flex-1"
			bind:value={query}
			aria-label="Search projects"
			placeholder="Search projects"
		/>
		<button class="btn join-item" type="submit">Search</button>
	</form>

	{#if data.error}
		<div role="alert" class="alert alert-error"><span>{data.error}</span></div>
	{:else if data.results.length === 0}
		<p class="text-base-content/60 text-sm">
			{data.q ? 'No projects match.' : 'Type a query to search.'}
		</p>
	{:else}
		<ul class="flex flex-col gap-3">
			{#each data.results as result (result.project_id)}
				<li class="card card-border">
					<div class="card-body">
						<a
							class="card-title link link-hover"
							href={`/p/${encodeURIComponent(result.project_id)}${homeParam()}`}
						>
							{result.display_name}
						</a>
						<p class="text-base-content/80 text-sm">{result.summary}</p>
						<span class="font-mono text-xs text-base-content/50">{shortDigest(result.project_id)}</span>
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</div>
