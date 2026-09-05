<script lang="ts">
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';

	import Digest from '$lib/components/Digest.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let query = $state(untrack(() => data.q));
	let game = $state(untrack(() => data.game));
	let loader = $state(untrack(() => data.loader));
	let gameVersion = $state(untrack(() => data.gameVersion));
	let loaderVersion = $state(untrack(() => data.loaderVersion));
	let runtimeVersion = $state(untrack(() => data.runtimeVersion));
	let channel = $state(untrack(() => data.channel));
	let platform = $state(untrack(() => data.platform));
	let sort = $state(untrack(() => data.sort || 'relevance'));

	const sorts = [
		['relevance', 'Relevance'],
		['updated', 'Recently updated'],
		['created', 'Recently added'],
		['popularity', 'Most followed here'],
		['name', 'Name'],
	] as const;

	function submit(event?: Event) {
		event?.preventDefault();
		const params = new URLSearchParams();
		if (query.trim()) params.set('q', query.trim());
		if (game.trim()) params.set('game', game.trim());
		if (loader.trim()) params.set('loader', loader.trim());
		if (gameVersion.trim()) params.set('game_version', gameVersion.trim());
		if (loaderVersion.trim()) params.set('loader_version', loaderVersion.trim());
		if (runtimeVersion.trim()) params.set('runtime_version', runtimeVersion.trim());
		if (channel.trim()) params.set('channel', channel.trim());
		if (platform.trim()) params.set('platform', platform.trim());
		if (sort && sort !== 'relevance') params.set('sort', sort);
		if (data.home) params.set('home', data.home);
		goto(`/search?${params.toString()}`);
	}

	function homeParam(): string {
		const params = new URLSearchParams();
		if (data.home) params.set('home', data.home);
		if (game) params.set('game', game);
		if (loader) params.set('loader', loader);
		if (sort && sort !== 'relevance') params.set('sort', sort);
		const encoded = params.toString();
		return encoded ? `?${encoded}` : '';
	}

	const activeFilters = $derived(
		[
			data.game,
			data.loader,
			data.gameVersion,
			data.loaderVersion,
			data.runtimeVersion,
			data.channel,
			data.platform,
		].filter((value) => value && value.length > 0).length,
	);
</script>

<svelte:head>
	<title>Search · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<div class="flex flex-col gap-2">
		<h1 class="text-2xl font-semibold tracking-tight">Find a mod</h1>
		<p class="max-w-2xl text-base-content/80">
			Search looks through the projects this instance has indexed. A project that is hidden here can
			still be opened from its own link, and a lower position never means a file is unsafe.
		</p>
	</div>

	<form class="flex w-full max-w-2xl flex-col gap-3" onsubmit={submit}>
		<div class="join w-full">
			<input
				class="input join-item flex-1"
				bind:value={query}
				aria-label="Search projects"
				placeholder="Search by name, summary, or release notes"
			/>
			<button class="btn btn-primary join-item" type="submit">Search</button>
		</div>

		<details class="rounded-box border border-base-300 bg-base-200 p-3">
			<summary class="cursor-pointer text-sm font-medium">
				Filters{activeFilters > 0 ? ` (${activeFilters} active)` : ''}
			</summary>
			<div class="mt-3 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
				<fieldset class="fieldset">
					<legend class="fieldset-legend">Game</legend>
					<input class="input input-sm" bind:value={game} placeholder="game id" />
				</fieldset>
				<fieldset class="fieldset">
					<legend class="fieldset-legend">Loader</legend>
					<input class="input input-sm" bind:value={loader} placeholder="loader id" />
				</fieldset>
				<fieldset class="fieldset">
					<legend class="fieldset-legend">Game version</legend>
					<input class="input input-sm" bind:value={gameVersion} placeholder="1.20.1" />
				</fieldset>
				<fieldset class="fieldset">
					<legend class="fieldset-legend">Loader version</legend>
					<input class="input input-sm" bind:value={loaderVersion} placeholder="0.15.0" />
				</fieldset>
				<fieldset class="fieldset">
					<legend class="fieldset-legend">Runtime version</legend>
					<input class="input input-sm" bind:value={runtimeVersion} placeholder="21" />
				</fieldset>
				<fieldset class="fieldset">
					<legend class="fieldset-legend">Channel</legend>
					<input class="input input-sm" bind:value={channel} placeholder="release" />
				</fieldset>
				<fieldset class="fieldset">
					<legend class="fieldset-legend">Platform</legend>
					<input class="input input-sm" bind:value={platform} placeholder="linux" />
				</fieldset>
				<fieldset class="fieldset">
					<legend class="fieldset-legend">Sort</legend>
					<select class="select select-sm" bind:value={sort} aria-label="Sort results">
						{#each sorts as [value, label] (value)}
							<option {value}>{label}</option>
						{/each}
					</select>
				</fieldset>
			</div>
			<p class="mt-2 text-xs text-base-content/60">
				Version filters match the versions a release declares. A loader version only applies when a
				loader is given.
			</p>
		</details>
	</form>

	{#if data.error}
		<div role="alert" class="alert alert-error alert-soft"><span>{data.error}</span></div>
	{:else if data.results.length === 0}
		<p class="text-base-content/60 text-sm">
			{data.q || activeFilters > 0
				? 'No projects match. Try a shorter query or fewer filters.'
				: 'Type a query or set a filter to start.'}
		</p>
	{:else}
		<ul class="flex flex-col gap-3">
			{#each data.results as result (result.project_id)}
				<li class="card card-border bg-base-200">
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
							{#if result.home}
								<span class="text-xs text-base-content/60">
									at <a
										class="link link-hover"
										href={`/p/${encodeURIComponent(result.project_id)}?home=${encodeURIComponent(result.home)}`}
										>{result.home}</a
									>
								</span>
							{/if}
							{#each result.annotations ?? [] as annotation (annotation.kind)}
								<span class="badge badge-warning badge-sm" role="note">{annotation.label}</span>
							{/each}
							{#if result.listing_state === 'withdrawn' || result.listing_state === 'unavailable'}
								<span class="badge badge-warning badge-sm" role="note">{result.listing_state}</span>
							{/if}
							{#if result.instance_popularity}
								<span class="badge badge-ghost badge-sm">
									{result.instance_popularity.value} downloads and follows, counted here
								</span>
							{/if}
						</div>
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</div>
