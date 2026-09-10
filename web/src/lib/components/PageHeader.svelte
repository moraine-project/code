<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		title,
		subtitle = '',
		children,
	}: { title: string; subtitle?: string; children?: Snippet } = $props();
</script>

<div class="flex flex-wrap items-end justify-between gap-4">
	<div class="flex flex-col gap-1">
		<h1 class="text-2xl font-bold tracking-tight sm:text-3xl">{title}</h1>
		{#if subtitle}
			<p class="max-w-2xl text-base-content/70">{subtitle}</p>
		{/if}
	</div>
	{#if children}
		<div class="flex items-center gap-2">{@render children()}</div>
	{/if}
</div>
