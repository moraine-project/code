<script lang="ts">
	import Avatar from '$lib/components/Avatar.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import GameCard from '$lib/components/GameCard.svelte';
	import PageHeader from '$lib/components/PageHeader.svelte';
	import SourceBadge from '$lib/components/SourceBadge.svelte';
	import { shortDigest } from '$lib/api/registry';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<svelte:head>
	<title>Browse games · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-8">
	<PageHeader
		title="Browse"
		subtitle="Pick a game to see what people have published for it. Each entry is a stable identity with a name attached."
	/>

	{#if data.error}
		<div role="alert" class="alert alert-error"><span>{data.error}</span></div>
	{/if}

	<section class="flex flex-col gap-4">
		<h2 class="text-lg font-semibold">Games</h2>
		{#if data.games.length === 0}
			<EmptyState
				title="No games yet"
				message="This instance has not published any game definitions. An operator adds them once, then every mod can target them."
			/>
		{:else}
			<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
				{#each data.games as game (game.id)}
					<GameCard {game} />
				{/each}
			</div>
		{/if}
	</section>

	<div class="grid gap-8 lg:grid-cols-2">
		<section class="flex flex-col gap-4">
			<h2 class="text-lg font-semibold">Loaders</h2>
			{#if data.loaders.length === 0}
				<p class="text-sm text-base-content/60">No loaders published yet.</p>
			{:else}
				<div class="flex flex-col gap-2">
					{#each data.loaders as loader (loader.id)}
						<a
							class="flex items-center gap-3 rounded-box border border-base-300 bg-base-200 p-3 transition hover:border-primary"
							href={`/search?loader=${encodeURIComponent(loader.id)}`}
						>
							<Avatar name={loader.display_name ?? 'Loader'} id={loader.id} size={36} />
							<div class="min-w-0">
								<p class="truncate font-medium">
									{loader.display_name ?? shortDigest(loader.id)}
								</p>
								<p class="text-xs text-base-content/60">
									{loader.current ? 'Ready' : 'No definition'}
								</p>
							</div>
						</a>
					{/each}
				</div>
			{/if}
		</section>

		<section class="flex flex-col gap-4">
			<h2 class="text-lg font-semibold">Runtimes</h2>
			{#if data.runtimes.length === 0}
				<p class="text-sm text-base-content/60">No runtimes published yet.</p>
			{:else}
				<div class="flex flex-col gap-2">
					{#each data.runtimes as runtime (runtime.id)}
						<div class="flex items-center gap-3 rounded-box border border-base-300 bg-base-200 p-3">
							<Avatar name={runtime.display_name ?? 'Runtime'} id={runtime.id} size={36} />
							<div class="min-w-0">
								<div class="flex items-center gap-2">
									<p class="truncate font-medium">
										{runtime.display_name ?? shortDigest(runtime.id)}
									</p>
									<SourceBadge sourceHome={runtime.source_home} />
								</div>
								<p class="text-xs text-base-content/60">
									{runtime.current ? 'Ready' : 'No definition'}
								</p>
							</div>
						</div>
					{/each}
				</div>
			{/if}
		</section>
	</div>
</div>
