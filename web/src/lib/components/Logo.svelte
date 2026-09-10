<script lang="ts">
	let { size = 28, wordmark = true }: { size?: number; wordmark?: boolean } = $props();
</script>

<a class="flex items-center gap-2" href="/" aria-label="Moraine home">
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
	{#if wordmark}
		<span class="text-lg font-semibold tracking-tight">Moraine</span>
	{/if}
</a>
