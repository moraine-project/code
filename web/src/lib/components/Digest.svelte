<script lang="ts">
	import { Check, Copy } from '@lucide/svelte';
	import { onDestroy } from 'svelte';
	import { shortDigest } from '$lib/api/digests';

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
		<code class="font-mono text-xs" title={value}>{shortDigest(value, length)}</code>
	{/if}
	<button
		class="btn btn-ghost btn-xs btn-square"
		type="button"
		onclick={copy}
		aria-label={`Copy ${label}`}
		title={copied ? 'Copied' : 'Copy'}
	>
		{#if copied}
			<Check size={13} class="text-success" />
		{:else}
			<Copy size={13} />
		{/if}
	</button>
</span>
