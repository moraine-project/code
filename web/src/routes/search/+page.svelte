<script lang="ts">
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';

	import { shortDigest } from '$lib/api/registry';
	import Digest from '$lib/components/Digest.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let query = $state(untrack(() => data.q));
	let game = $state(untrack(() => data.game));
	let loader = $state(untrack(() => data.loader));
	let category = $state(untrack(() => data.category));
	let tag = $state(untrack(() => data.tag));
	let gameVersion = $state(untrack(() => data.gameVersion));
	let loaderVersion = $state(untrack(() => data.loaderVersion));
	let runtimeVersion = $state(untrack(() => data.runtimeVersion));
	let channel = $state(untrack(() => data.channel));
	let platform = $state(untrack(() => data.platform));
	let sort = $state(untrack(() => data.sort || 'relevance'));

	const sorts = [
		['relevance', 'Best match'],
		['updated', 'Recently updated'],
		['created', 'Recently added'],
		['popularity', 'Most followed here'],
		['name', 'Name'],
	] as const;

	function submit() {
		const params = new URLSearchParams();
		if (query.trim()) params.set('q', query.trim());
		if (game.trim()) params.set('game', game.trim());
		if (loader.trim()) params.set('loader', loader.trim());
		if (category.trim()) params.set('category', category.trim());
		if (tag.trim()) params.set('tag', tag.trim());
		if (gameVersion.trim()) params.set('game_version', gameVersion.trim());
		if (loaderVersion.trim()) params.set('loader_version', loaderVersion.trim());
		if (runtimeVersion.trim()) params.set('runtime_version', runtimeVersion.trim());
		if (channel.trim()) params.set('channel', channel.trim());
		if (platform.trim()) params.set('platform', platform.trim());
		if (sort && sort !== 'relevance') params.set('sort', sort);
		if (data.home) params.set('home', data.home);
		goto(`/search?${params.toString()}`);
	}

	function reset() {
		query = '';
		game = '';
		loader = '';
		category = '';
		tag = '';
		gameVersion = '';
		loaderVersion = '';
		runtimeVersion = '';
		channel = '';
		platform = '';
		sort = 'relevance';
		submit();
	}

	const current = $derived<Record<string, string>>({
		game,
		loader,
		category,
		tag,
		game_version: gameVersion,
		loader_version: loaderVersion,
		runtime_version: runtimeVersion,
		channel,
		platform,
	});

	const facetGroups = $derived(
		[
			{ title: 'Game', dimension: 'game', values: data.facets.game },
			{ title: 'Loader', dimension: 'loader', values: data.facets.loader },
			{ title: 'Category', dimension: 'category', values: data.facets.category },
			{ title: 'Tag', dimension: 'tag', values: data.facets.tag },
			{ title: 'Game version', dimension: 'game_version', values: data.facets.game_version },
			{ title: 'Loader version', dimension: 'loader_version', values: data.facets.loader_version },
			{
				title: 'Runtime version',
				dimension: 'runtime_version',
				values: data.facets.runtime_version,
			},
			{ title: 'Channel', dimension: 'channel', values: data.facets.channel },
			{ title: 'Platform', dimension: 'platform', values: data.facets.platform },
		].filter((group) => group.values.length > 0),
	);

	const gameNames = $derived(
		new Map(data.games.map((entry) => [entry.id, entry.display_name ?? entry.id])),
	);
	const loaderNames = $derived(
		new Map(data.loaders.map((entry) => [entry.id, entry.display_name ?? entry.id])),
	);

	function labelFor(dimension: string, value: string): string {
		if (dimension === 'game') return gameNames.get(value) ?? shortDigest(value, 18);
		if (dimension === 'loader') return loaderNames.get(value) ?? shortDigest(value, 18);
		if (dimension === 'loader_version') {
			const [id, version] = value.split('\u001f');
			return `${labelFor('loader', id)} ${version}`;
		}
		return value;
	}

	function applyFacet(dimension: string, value: string) {
		const next = current[dimension] === value ? '' : value;
		switch (dimension) {
			case 'game':
				game = next;
				break;
			case 'loader':
				loader = next;
				break;
			case 'category':
				category = next;
				break;
			case 'tag':
				tag = next;
				break;
			case 'game_version':
				gameVersion = next;
				break;
			case 'loader_version':
				loaderVersion = next;
				break;
			case 'runtime_version':
				runtimeVersion = next;
				break;
			case 'channel':
				channel = next;
				break;
			case 'platform':
				platform = next;
				break;
		}
		submit();
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

	const searched = $derived(
		Boolean(data.q) || Object.values(current).some((value) => value.length > 0),
	);
</script>

<svelte:head>
	<title>Find a mod · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<div class="flex flex-col gap-2">
		<h1 class="text-2xl font-semibold tracking-tight">Find a mod</h1>
		<p class="max-w-2xl text-base-content/80">
			Search looks through the projects this instance has indexed. A project that is hidden here can
			still be opened from its own link, and a lower position never means a file is unsafe.
		</p>
	</div>

	<form
		class="flex w-full max-w-2xl flex-col gap-3"
		onsubmit={(event) => {
			event.preventDefault();
			submit();
		}}
	>
		<div class="join w-full">
			<input
				class="input join-item flex-1"
				bind:value={query}
				aria-label="Search projects"
				placeholder="Search by name, summary, or release notes"
			/>
			<button class="btn btn-primary join-item" type="submit">Search</button>
		</div>
	</form>

	<datalist id="game-options">
		{#each data.games as option (option.id)}
			<option value={option.id}>{option.display_name ?? option.id}</option>
		{/each}
	</datalist>
	<datalist id="loader-options">
		{#each data.loaders as option (option.id)}
			<option value={option.id}>{option.display_name ?? option.id}</option>
		{/each}
	</datalist>

	<div class="grid gap-6 lg:grid-cols-[18rem_1fr]">
		<aside class="lg:sticky lg:top-20 lg:self-start">
			<div class="card card-border bg-base-200">
				<div class="card-body gap-4">
					<div class="flex items-center justify-between">
						<h2 class="card-title text-base">Filters</h2>
						<button class="btn btn-ghost btn-xs" type="button" onclick={reset}>Reset</button>
					</div>

					{#each facetGroups as group (group.dimension)}
						<div class="flex flex-col gap-1">
							<p class="text-xs font-medium uppercase tracking-wide text-base-content/60">
								{group.title}
							</p>
							<div class="flex flex-wrap gap-1">
								{#each group.values as entry (entry.value)}
									<button
										class="btn btn-xs gap-1 {current[group.dimension] === entry.value
											? 'btn-primary'
											: 'btn-ghost border-base-300'}"
										type="button"
										aria-pressed={current[group.dimension] === entry.value}
										onclick={() => applyFacet(group.dimension, entry.value)}
									>
										{labelFor(group.dimension, entry.value)}
										<span class="badge badge-xs">{entry.count}</span>
									</button>
								{/each}
							</div>
						</div>
					{/each}

					{#if facetGroups.length === 0}
						<p class="text-sm text-base-content/60">No filters are available from this instance.</p>
					{/if}

					<details class="rounded-box border border-base-300 bg-base-100 p-2">
						<summary class="cursor-pointer text-sm font-medium">Type a value</summary>
						<div class="mt-2 flex flex-col gap-2">
							<label class="fieldset">
								<span class="label">Game</span>
								<input
									class="input input-sm"
									bind:value={game}
									list="game-options"
									placeholder="game id"
								/>
							</label>
							<label class="fieldset">
								<span class="label">Loader</span>
								<input
									class="input input-sm"
									bind:value={loader}
									list="loader-options"
									placeholder="loader id"
								/>
							</label>
							<label class="fieldset">
								<span class="label">Category</span>
								<input class="input input-sm" bind:value={category} placeholder="category id" />
							</label>
							<label class="fieldset">
								<span class="label">Tag</span>
								<input class="input input-sm" bind:value={tag} placeholder="tag id" />
							</label>
							<label class="fieldset">
								<span class="label">Game version</span>
								<input class="input input-sm" bind:value={gameVersion} placeholder="1.20.1" />
							</label>
							<label class="fieldset">
								<span class="label">Loader version</span>
								<input class="input input-sm" bind:value={loaderVersion} placeholder="0.15.0" />
							</label>
							<label class="fieldset">
								<span class="label">Runtime version</span>
								<input class="input input-sm" bind:value={runtimeVersion} placeholder="21" />
							</label>
							<label class="fieldset">
								<span class="label">Channel</span>
								<input class="input input-sm" bind:value={channel} placeholder="release" />
							</label>
							<label class="fieldset">
								<span class="label">Platform</span>
								<input class="input input-sm" bind:value={platform} placeholder="linux" />
							</label>
							<label class="fieldset">
								<span class="label">Sort by</span>
								<select class="select select-sm" bind:value={sort} aria-label="Sort results">
									{#each sorts as [value, label] (value)}
										<option {value}>{label}</option>
									{/each}
								</select>
							</label>
							<button class="btn btn-sm" type="button" onclick={submit}>Apply filters</button>
						</div>
					</details>

					<p class="text-xs text-base-content/60">
						Counts describe this instance's own index, with the other filters applied. A version
						range is matched on the release feed, not here.
					</p>
				</div>
			</div>
		</aside>

		<section class="flex flex-col gap-3">
			<p class="text-sm text-base-content/70" role="status">
				{#if data.error}
					The search could not run.
				{:else if !searched}
					Type a query or set a filter to start.
				{:else if data.results.length === 0}
					No projects match. Try a shorter query or fewer filters.
				{:else}
					Showing {data.results.length}
					{data.results.length === 1 ? 'project' : 'projects'}{data.q ? ` for “${data.q}”` : ''} from
					this instance's index.
				{/if}
			</p>

			{#if data.error}
				<div role="alert" class="alert alert-error alert-soft"><span>{data.error}</span></div>
			{:else}
				<ul class="flex flex-col gap-3">
					{#each data.results as result (result.project_id)}
						<li class="card card-border bg-base-200">
							<div class="card-body gap-2">
								<div class="flex flex-wrap items-start justify-between gap-3">
									<div class="flex flex-col gap-1">
										<a
											class="card-title link link-hover"
											href={`/p/${encodeURIComponent(result.project_id)}${homeParam()}`}
										>
											{result.display_name}
										</a>
										{#if result.summary}
											<p class="max-w-2xl text-base-content/80 text-sm">{result.summary}</p>
										{/if}
									</div>
									<div class="flex items-center gap-2">
										{#if result.listing_state === 'withdrawn' || result.listing_state === 'unavailable'}
											<span class="badge badge-warning badge-sm" role="note"
												>{result.listing_state}</span
											>
										{/if}
										<a
											class="btn btn-primary btn-sm"
											href={`/p/${encodeURIComponent(result.project_id)}${homeParam()}`}
										>
											Open project
										</a>
									</div>
								</div>
								<div class="flex flex-wrap items-center gap-2">
									<Digest copyOnly value={result.project_id} label="the project id" />
									{#if result.home}
										<span class="text-xs text-base-content/60">
											hosted at <a
												class="link link-hover"
												href={`/p/${encodeURIComponent(result.project_id)}?home=${encodeURIComponent(result.home)}`}
												>{result.home}</a
											>
										</span>
									{/if}
									{#if result.instance_popularity}
										<span class="badge badge-ghost badge-sm">
											{result.instance_popularity.value} downloads and follows, counted here
										</span>
									{/if}
								</div>
								{#each result.annotations ?? [] as annotation (annotation.kind)}
									<div role="note" class="alert alert-warning alert-soft py-2 text-sm">
										<span>{annotation.label}</span>
									</div>
								{/each}
							</div>
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	</div>
</div>
