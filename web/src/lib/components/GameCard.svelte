<script lang="ts">
	import type { DefinitionSummary } from '$lib/api/registry';
	import Avatar from './Avatar.svelte';

	let { game, count = null }: { game: DefinitionSummary; count?: number | null } = $props();
</script>

<a
	class="card card-border bg-base-200 transition hover:border-primary hover:bg-base-300/60"
	href={`/search?game=${encodeURIComponent(game.id)}`}
>
	<div class="card-body flex-row items-center gap-4">
		<Avatar name={game.display_name ?? 'Game'} id={game.id} size={48} />
		<div class="min-w-0 flex-1">
			<p class="truncate font-semibold">{game.display_name ?? 'Unnamed game'}</p>
			<p class="text-xs text-base-content/60">
				{#if count !== null}
					{count} {count === 1 ? 'mod' : 'mods'}
				{:else if game.current}
					Browse mods
				{:else}
					No definition published yet
				{/if}
			</p>
		</div>
		<svg
			class="h-5 w-5 shrink-0 opacity-40"
			viewBox="0 0 24 24"
			fill="none"
			stroke="currentColor"
			stroke-width="2"
			stroke-linecap="round"
			aria-hidden="true"
		>
			<path d="m9 6 6 6-6 6" />
		</svg>
	</div>
</a>
