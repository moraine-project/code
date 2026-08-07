<script lang="ts">
	import { onDestroy } from 'svelte';
	import { shortDigest } from '$lib/api/registry';

	let {
		value,
		label = 'digest',
		length = 16,
		copyOnly = false,
	}: { value: string; label?: string; length?: number; copyOnly?: boolean } = $props();
	let copied = $state(false);
	let timer: ReturnType<typeof setTimeout> | undefined;

	onDestroy(() => clearTimeout(timer));

	async function copy() {
		try {
			await navigator.clipboard.writeText(value);
			copied = true;
			clearTimeout(timer);
			timer = setTimeout(() => (copied = false), 2000);
		} catch {
			copied = false;
		}
	}
</script>

<span class="inline-flex items-center gap-1">
	{#if !copyOnly}
		<code class="font-mono" title={value}>{shortDigest(value, length)}</code>
	{/if}
	<button class="btn btn-ghost btn-xs" type="button" onclick={copy} aria-label={`Copy ${label}`}>
		{copied ? 'copied' : 'copy'}
	</button>
</span>
