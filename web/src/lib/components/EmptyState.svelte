<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		title,
		message = '',
		children,
	}: { title: string; message?: string; children?: Snippet } = $props();
</script>

<div
	class="flex flex-col items-center gap-3 rounded-box border border-dashed border-base-300 p-10 text-center"
>
	<p class="font-medium">{title}</p>
	{#if message}
		<p class="max-w-md text-sm text-base-content/60">{message}</p>
	{/if}
	{#if children}
		<div class="mt-1">{@render children()}</div>
	{/if}
</div>
