<script lang="ts">
	import { shortDigest } from '$lib/api/registry';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<svelte:head>
	<title>Browse games and loaders · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-8">
	<div class="flex flex-col gap-2">
		<h1 class="text-2xl font-semibold tracking-tight">Browse</h1>
		<p class="max-w-2xl text-base-content/80">
			Games, loaders, and runtimes that this instance knows about. Pick a game to see the mods
			published for it. A name is never proof of identity; each entry has a stable ID you can check.
		</p>
	</div>

	{#if data.error}
		<div role="alert" class="alert alert-error alert-soft"><span>{data.error}</span></div>
	{:else}
		<section class="flex flex-col gap-3">
			<h2 class="text-lg font-semibold">Games</h2>
			{#if data.games.length === 0}
				<p class="text-base-content/60 text-sm">No games are hosted here yet.</p>
			{:else}
				<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
					{#each data.games as game (game.id)}
						<div class="card card-border bg-base-200">
							<div class="card-body gap-2">
								<h3 class="card-title text-base">
									{game.display_name ?? shortDigest(game.id)}
								</h3>
								<span class="badge badge-ghost badge-sm w-fit">
									{game.current ? 'definition published' : 'no definition yet'}
								</span>
								<div class="card-actions mt-1">
									<a class="btn btn-sm" href={`/search?game=${encodeURIComponent(game.id)}`}>
										See mods
									</a>
									<a
										class="btn btn-ghost btn-sm"
										href={`/definitions/games/${encodeURIComponent(game.id)}`}
									>
										Details
									</a>
								</div>
							</div>
						</div>
					{/each}
				</div>
			{/if}
		</section>

		<section class="flex flex-col gap-3">
			<h2 class="text-lg font-semibold">Loaders</h2>
			{#if data.loaders.length === 0}
				<p class="text-base-content/60 text-sm">No loaders are hosted here yet.</p>
			{:else}
				<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
					{#each data.loaders as loader (loader.id)}
						<div class="card card-border bg-base-200">
							<div class="card-body gap-2">
								<h3 class="card-title text-base">
									{loader.display_name ?? shortDigest(loader.id)}
								</h3>
								<span class="badge badge-ghost badge-sm w-fit">
									{loader.current ? 'definition published' : 'no definition yet'}
								</span>
								<div class="card-actions mt-1">
									<a class="btn btn-sm" href={`/search?loader=${encodeURIComponent(loader.id)}`}>
										See mods
									</a>
									<a
										class="btn btn-ghost btn-sm"
										href={`/definitions/loaders/${encodeURIComponent(loader.id)}`}
									>
										Details
									</a>
								</div>
							</div>
						</div>
					{/each}
				</div>
			{/if}
		</section>

		<section class="flex flex-col gap-3">
			<h2 class="text-lg font-semibold">Runtimes</h2>
			{#if data.runtimes.length === 0}
				<p class="text-base-content/60 text-sm">No runtimes are hosted here yet.</p>
			{:else}
				<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
					{#each data.runtimes as runtime (runtime.id)}
						<div class="card card-border bg-base-200">
							<div class="card-body gap-2">
								<h3 class="card-title text-base">
									{runtime.display_name ?? shortDigest(runtime.id)}
								</h3>
								<span class="badge badge-ghost badge-sm w-fit">
									{runtime.current ? 'definition published' : 'no definition yet'}
								</span>
								<div class="card-actions mt-1">
									<a
										class="btn btn-ghost btn-sm"
										href={`/definitions/runtimes/${encodeURIComponent(runtime.id)}`}
									>
										Details
									</a>
								</div>
							</div>
						</div>
					{/each}
				</div>
			{/if}
		</section>
	{/if}
</div>
