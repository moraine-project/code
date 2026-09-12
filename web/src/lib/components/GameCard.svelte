<script lang="ts">
	import type { DefinitionSummary } from '$lib/api/registry';
	import { ChevronRight } from '@lucide/svelte';
	import Avatar from './Avatar.svelte';
	import SourceBadge from './SourceBadge.svelte';

	let { game, count = null }: { game: DefinitionSummary; count?: number | null } = $props();
</script>

<a
	class="card card-border bg-base-200 transition hover:border-primary hover:bg-base-300/60"
	href={`/search?game=${encodeURIComponent(game.id)}`}
>
	<div class="card-body flex-row items-center gap-4">
		<Avatar name={game.display_name ?? 'Game'} id={game.id} size={48} />
		<div class="min-w-0 flex-1">
			<div class="flex items-center gap-2">
				<p class="truncate font-semibold">{game.display_name ?? 'Unnamed game'}</p>
				<SourceBadge sourceHome={game.source_home} />
			</div>
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
		<ChevronRight size={20} class="shrink-0 opacity-40" />
	</div>
</a>
