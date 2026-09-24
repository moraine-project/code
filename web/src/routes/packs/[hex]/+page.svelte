<script lang="ts">
	import EmptyState from '$lib/components/EmptyState.svelte';
	import PageHeader from '$lib/components/PageHeader.svelte';
	import { pageTitle } from '$lib/title.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<svelte:head><title>{pageTitle('Modpack manifest')}</title></svelte:head>

<div class="flex flex-col gap-6">
	<PageHeader
		title="Modpack manifest"
		subtitle="A signed manifest of the exact projects and files in a pack."
	/>
	{#if data.error}
		<div role="alert" class="alert alert-error"><span>{data.error}</span></div>
	{:else if !data.pack}
		<EmptyState
			title="Manifest not found"
			message="This home does not have that modpack manifest."
		/>
	{:else}
		<section class="card card-border bg-base-200">
			<div class="card-body gap-3">
				<p class="font-mono text-xs break-all">{data.pack.pack}</p>
				<p class="text-sm text-base-content/60">Published by {data.home}</p>
				<pre class="overflow-x-auto rounded-box bg-base-300 p-4 text-xs">{JSON.stringify(
						data.pack.payload,
						null,
						2,
					)}</pre>
			</div>
		</section>
	{/if}
</div>
