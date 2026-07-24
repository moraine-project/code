<script lang="ts">
	import { shortDigest } from '$lib/api/registry';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<svelte:head>
	<title>Browse games and loaders · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<h1 class="text-2xl font-bold">Browse</h1>
	<p class="text-base-content/80 max-w-2xl">
		These are the game, loader, and runtime identities this directory serves. A definition is
		trusted only when it verifies against its own pinned identity, not because it is listed here.
	</p>

	{#if data.error}
		<div role="alert" class="alert alert-error"><span>{data.error}</span></div>
	{:else}
		<section class="flex flex-col gap-3">
			<h2 class="text-xl font-semibold">Games</h2>
			{#if data.games.length === 0}
				<p class="text-base-content/60 text-sm">No games are hosted here yet.</p>
			{:else}
				<ul class="flex flex-wrap gap-2">
					{#each data.games as game (game.id)}
						<li>
							<a class="btn btn-outline" href={`/search?game=${encodeURIComponent(game.id)}`}>
								{game.display_name ?? shortDigest(game.id)}
								<span class="badge badge-sm">{game.current ? 'defined' : 'no definition'}</span>
							</a>
						</li>
					{/each}
				</ul>
			{/if}
		</section>

		<section class="flex flex-col gap-3">
			<h2 class="text-xl font-semibold">Loaders</h2>
			{#if data.loaders.length === 0}
				<p class="text-base-content/60 text-sm">No loaders are hosted here yet.</p>
			{:else}
				<ul class="flex flex-wrap gap-2">
					{#each data.loaders as loader (loader.id)}
						<li class="badge badge-outline badge-lg">
							{loader.display_name ?? shortDigest(loader.id)}
							<span class="badge badge-sm">{loader.current ? 'defined' : 'no definition'}</span>
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	{/if}
</div>
