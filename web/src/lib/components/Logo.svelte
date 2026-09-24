<script lang="ts">
	import { home } from '$lib/home.svelte';
	import { instance } from '$lib/instance.svelte';

	let { size = 28, wordmark = true }: { size?: number; wordmark?: boolean } = $props();
</script>

<a class="flex items-center gap-2" href={home.url('/')} aria-label={`${instance.name} home`}>
	{#if instance.logoUrl}
		<img src={instance.logoUrl} width={size} height={size} alt="" role="presentation" />
	{:else}
		<svg
			viewBox="0 0 32 32"
			width={size}
			height={size}
			class="text-primary"
			role="img"
			aria-hidden="true"
		>
			<path d="M2 26 L11 8 L16 16 L21 8 L30 26 Z" fill="currentColor" />
			<circle cx="25.5" cy="8" r="2.4" fill="currentColor" opacity="0.55" />
		</svg>
	{/if}
	{#if wordmark}
		<span class="text-lg font-semibold tracking-tight">{instance.name}</span>
	{/if}
</a>
