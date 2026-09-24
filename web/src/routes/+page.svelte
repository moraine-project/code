<script lang="ts">
	import GameCard from '$lib/components/GameCard.svelte';
	import ProjectCard from '$lib/components/ProjectCard.svelte';
	import SearchBar from '$lib/components/SearchBar.svelte';
	import { shortDigest } from '$lib/api/digests';
	import { home } from '$lib/home.svelte';
	import { pageTitle } from '$lib/title.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const gameNames = $derived(
		new Map(data.games.map((game) => [game.id, game.display_name ?? shortDigest(game.id)])),
	);
</script>

<svelte:head>
	<title>{pageTitle('publish and find game mods')}</title>
</svelte:head>

<div class="flex flex-col gap-12">
	<section
		class="relative overflow-hidden rounded-box border border-base-300 bg-base-200 px-6 py-14 sm:px-10 sm:py-20"
	>
		<div
			class="pointer-events-none absolute inset-0 opacity-40"
			style="background:radial-gradient(60% 80% at 80% 0%, color-mix(in oklab, var(--color-primary) 30%, transparent), transparent 70%)"
		></div>
		<div class="relative flex max-w-2xl flex-col gap-5">
			<h1 class="text-3xl font-bold tracking-tight sm:text-5xl">Mods for the games you play</h1>
			<p class="text-lg text-base-content/70">
				Discover, download, and publish mods, plugins, and packs. Every project stays at a home its
				publisher controls, so it keeps working even when a directory stops listing it.
			</p>
			<div class="pt-1">
				<SearchBar />
			</div>
			<div class="flex flex-wrap gap-4 pt-2 text-sm text-base-content/70">
				<span><strong class="text-base-content">{data.games.length}</strong> games</span>
				<span><strong class="text-base-content">{data.recent.length}</strong> recently updated</span
				>
			</div>
		</div>
	</section>

	{#if data.error}
		<div role="alert" class="alert alert-error"><span>{data.error}</span></div>
	{/if}

	{#if data.games.length > 0}
		<section class="flex flex-col gap-4">
			<div class="flex items-center justify-between">
				<h2 class="text-xl font-semibold">Popular games</h2>
				<a class="link link-hover text-sm" href={home.url('/games')}>Browse all</a>
			</div>
			<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
				{#each data.games.slice(0, 6) as game (game.id)}
					<GameCard {game} />
				{/each}
			</div>
		</section>
	{/if}

	{#if data.recent.length > 0}
		<section class="flex flex-col gap-4">
			<div class="flex items-center justify-between">
				<h2 class="text-xl font-semibold">Recently updated</h2>
				<a class="link link-hover text-sm" href={home.url('/search?sort=updated')}>See more</a>
			</div>
			<div class="grid gap-3 lg:grid-cols-2">
				{#each data.recent as result (result.project_id)}
					<ProjectCard {result} gameName={gameNames.get(result.game_id) ?? ''} />
				{/each}
			</div>
		</section>
	{/if}

	{#if data.popular.length > 0}
		<section class="flex flex-col gap-4">
			<div class="flex items-center justify-between">
				<h2 class="text-xl font-semibold">Popular on this instance</h2>
				<a class="link link-hover text-sm" href={home.url('/search?sort=popularity')}>See more</a>
			</div>
			<div class="grid gap-3 lg:grid-cols-2">
				{#each data.popular as result (result.project_id)}
					<ProjectCard {result} gameName={gameNames.get(result.game_id) ?? ''} />
				{/each}
			</div>
		</section>
	{/if}

	{#if data.recent.length === 0 && !data.error}
		<section
			class="flex flex-col items-center gap-3 rounded-box border border-dashed border-base-300 p-10 text-center"
		>
			<h2 class="text-xl font-semibold">Nothing published yet</h2>
			<p class="max-w-md text-base-content/70">
				This instance is empty. If you make mods, you can be the first to publish here.
			</p>
			<a class="btn btn-primary" href="/publish">Publish a mod</a>
		</section>
	{/if}

	<section
		class="flex flex-wrap items-center justify-between gap-4 rounded-box border border-base-300 bg-base-200 px-6 py-8"
	>
		<div class="flex flex-col gap-1">
			<h2 class="text-xl font-semibold">Make something? Ship it here.</h2>
			<p class="text-base-content/70">
				Sign in, pick a game, upload a file, and publish. No command line required.
			</p>
		</div>
		<a class="btn btn-primary" href="/publish">Start publishing</a>
	</section>
</div>
