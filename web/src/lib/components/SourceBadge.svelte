<script lang="ts">
	let { sourceHome = null }: { sourceHome?: string | null } = $props();
</script>

{#if sourceHome}
	<span class="badge badge-outline badge-sm" title={`Pulled from ${sourceHome}`}>Federated</span>
{:else}
	<span class="badge badge-ghost badge-sm" title="Authored or imported on this instance">Local</span
	>
{/if}
