<script lang="ts">
	import { shortDigest } from '$lib/api/registry';
	import Digest from '$lib/components/Digest.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const totals = $derived([
		['Games', data.games.length],
		['Loaders', data.loaders.length],
		['Runtimes', data.runtimes.length],
	] as const);

	function status(current: string | null | undefined): string {
		return current ? 'Ready to install' : 'Not set up here yet';
	}
</script>

<svelte:head>
	<title>Browse games and loaders · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-8">
	<section class="flex flex-col gap-4 rounded-box border border-base-300 bg-base-200 p-6">
		<div class="flex flex-col gap-2">
			<h1 class="text-2xl font-semibold tracking-tight">Browse games and loaders</h1>
			<p class="max-w-2xl text-base-content/80">
				Pick a game to see the mods published for it. Each entry is a stable identity with a name
				attached, so an unfamiliar name is never mistaken for a familiar one.
			</p>
		</div>
		<div class="stats stats-vertical bg-base-100 sm:stats-horizontal">
			{#each totals as [label, value] (label)}
				<div class="stat">
					<div class="stat-title">{label}</div>
					<div class="stat-value text-2xl">{value}</div>
				</div>
			{/each}
		</div>
	</section>

	{#if data.error}
		<div role="alert" class="alert alert-error alert-soft"><span>{data.error}</span></div>
	{:else}
		<section class="flex flex-col gap-3">
			<div class="flex flex-col gap-1">
				<h2 class="text-lg font-semibold">Games</h2>
				<p class="text-sm text-base-content/70">Games this instance can install mods for.</p>
			</div>
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
								<span class="badge badge-ghost badge-sm w-fit">{status(game.current)}</span>
								<Digest copyOnly value={game.id} label="the game id" />
								<div class="card-actions mt-1">
									<a
										class="btn btn-primary btn-sm"
										href={`/search?game=${encodeURIComponent(game.id)}`}
									>
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
			<div class="flex flex-col gap-1">
				<h2 class="text-lg font-semibold">Loaders</h2>
				<p class="text-sm text-base-content/70">
					Loaders that add mod support to a game. A release declares which loader it needs.
				</p>
			</div>
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
								<span class="badge badge-ghost badge-sm w-fit">{status(loader.current)}</span>
								<Digest copyOnly value={loader.id} label="the loader id" />
								<div class="card-actions mt-1">
									<a
										class="btn btn-primary btn-sm"
										href={`/search?loader=${encodeURIComponent(loader.id)}`}
									>
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
			<div class="flex flex-col gap-1">
				<h2 class="text-lg font-semibold">Runtimes</h2>
				<p class="text-sm text-base-content/70">
					The software a loader or mod runs on, such as a Java or .NET version.
				</p>
			</div>
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
								<span class="badge badge-ghost badge-sm w-fit">{status(runtime.current)}</span>
								<Digest copyOnly value={runtime.id} label="the runtime id" />
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

		<section class="card card-border bg-base-200">
			<div class="card-body flex-row flex-wrap items-center justify-between gap-4">
				<div class="flex flex-col gap-1">
					<h2 class="card-title text-base">Missing a game or loader?</h2>
					<p class="text-sm text-base-content/70">
						An operator can add one from a readable definition file, and publish it so other
						instances can pin it.
					</p>
				</div>
				<a class="btn btn-primary" href="/publish">Publish a definition</a>
			</div>
		</section>
	{/if}
</div>
