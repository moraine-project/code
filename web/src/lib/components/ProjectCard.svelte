<script lang="ts">
	import type { SearchResult } from '$lib/api/registry';
	import { TrendingUp } from '@lucide/svelte';
	import Avatar from './Avatar.svelte';

	let { result, gameName = '' }: { result: SearchResult; gameName?: string } = $props();
</script>

<a
	class="card card-border bg-base-200 transition hover:border-primary hover:bg-base-300/60"
	href={`/p/${encodeURIComponent(result.project_id)}`}
>
	<div class="card-body flex-row items-start gap-4">
		<Avatar name={result.display_name} id={result.project_id} size={56} />
		<div class="flex min-w-0 flex-1 flex-col gap-1">
			<div class="flex flex-wrap items-center gap-2">
				<p class="truncate font-semibold">{result.display_name}</p>
				{#if result.listing_state !== 'listed'}
					<span class="badge badge-warning badge-sm">{result.listing_state}</span>
				{/if}
			</div>
			{#if result.summary}
				<p class="line-clamp-2 text-sm text-base-content/70">{result.summary}</p>
			{/if}
			<div class="mt-1 flex flex-wrap items-center gap-2 text-xs text-base-content/60">
				{#if gameName}
					<span class="badge badge-ghost badge-sm">{gameName}</span>
				{/if}
				{#if result.instance_popularity}
					<span class="inline-flex items-center gap-1">
						<TrendingUp size={14} />
						{result.instance_popularity.value}
					</span>
				{/if}
			</div>
			{#each result.annotations as note (note.label)}
				<p class="text-xs text-warning">{note.label}</p>
			{/each}
		</div>
	</div>
</a>
