<script lang="ts">
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';

	import Digest from '$lib/components/Digest.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let query = $state(untrack(() => data.q));
	let sort = $state(untrack(() => data.sort || 'relevance'));

	const sorts = [
		['relevance', 'Relevance'],
		['updated', 'Recently updated'],
		['created', 'Recently added'],
		['popularity', 'Most downloaded here'],
		['name', 'Name'],
	] as const;

	function sortParam(): string {
		return sort === 'relevance' ? '' : sort;
	}

	function submit(event: Event) {
		event.preventDefault();
		const params = new URLSearchParams();
		if (query.trim().length > 0) params.set('q', query.trim());
		if (data.game) params.set('game', data.game);
		if (data.loader) params.set('loader', data.loader);
		if (sortParam()) params.set('sort', sortParam());
		if (data.home) params.set('home', data.home);
		goto(`/search?${params.toString()}`);
	}

	function homeParam(): string {
		const params = new URLSearchParams();
		if (data.home) params.set('home', data.home);
		if (data.game) params.set('game', data.game);
		if (data.loader) params.set('loader', data.loader);
		if (sort) params.set('sort', sort);
		const encoded = params.toString();
		return encoded ? `?${encoded}` : '';
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

	<label class="form-control w-fit">
		<span class="label-text">Sort</span>
		<select
			class="select select-bordered select-sm"
			bind:value={sort}
			onchange={submit}
			aria-label="Sort results"
		>
			{#each sorts as [value, label] (value)}
				<option {value}>{label}</option>
			{/each}
		</select>
	</label>

	{#if data.error}
		<div role="alert" class="alert alert-error"><span>{data.error}</span></div>
	{:else if data.results.length === 0}
		<p class="text-base-content/60 text-sm">
			{data.q || data.game || data.loader ? 'No projects match.' : 'Type a query to search.'}
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
						<div class="flex flex-wrap items-center gap-2">
							<Digest value={result.project_id} label="the project id" length={12} />
							{#each result.annotations ?? [] as annotation (annotation.kind)}
								<span class="badge badge-warning badge-sm" role="note">{annotation.label}</span>
							{/each}
							{#if result.instance_popularity}
								<span class="badge badge-ghost badge-sm">
									{result.instance_popularity.value} downloads and follows, this instance
								</span>
							{/if}
						</div>
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</div>
