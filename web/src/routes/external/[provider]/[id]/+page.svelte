<script lang="ts">
	import { safeExternalUrl, shortDigest } from '$lib/api/registry';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	function formatDate(seconds: number): string {
		return new Date(seconds * 1000).toLocaleDateString(undefined, {
			year: 'numeric',
			month: 'short',
			day: 'numeric',
		});
	}

	const sourceUrl = $derived(
		data.project ? safeExternalUrl(data.project.canonical_source_url) : null,
	);
</script>

<svelte:head>
	<title
		>{data.project
			? `${data.project.observed_profile.name ?? data.project.external_project_id} · External project`
			: 'External project · Moraine'}</title
	>
</svelte:head>

<div class="flex flex-col gap-6">
	{#if data.error}
		<div role="alert" class="alert alert-error">{data.error}</div>
	{:else if data.project}
		<section
			class="flex flex-col gap-4 rounded-box border border-warning/40 bg-warning/10 px-6 py-8"
		>
			<div class="flex flex-wrap items-center gap-2 text-sm text-warning-content/80">
				<span class="badge badge-warning">External catalog</span>
				<span>{data.project.provider} · {data.project.external_project_id}</span>
			</div>
			<div>
				<h1 class="text-3xl font-bold tracking-tight">
					{data.project.observed_profile.name ?? data.project.external_project_id}
				</h1>
				<p class="mt-2 text-base-content/70">
					This is an observed record from another platform, not a Moraine-signed project.
				</p>
			</div>
			{#if sourceUrl}
				<a class="btn btn-sm w-fit" href={sourceUrl} target="_blank" rel="noopener noreferrer"
					>Open source</a
				>
			{/if}
		</section>

		<section class="card card-border bg-base-200">
			<div class="card-body gap-4">
				<div class="flex flex-wrap items-center justify-between gap-2">
					<h2 class="card-title">Observed files</h2>
					<span class="text-sm text-base-content/60"
						>Updated {formatDate(data.project.last_synced_at)}</span
					>
				</div>
				{#if data.project.files.length === 0}
					<p class="text-base-content/70">No files were recorded by the bridge.</p>
				{:else}
					<div class="overflow-x-auto">
						<table class="table table-sm">
							<thead><tr><th>File</th><th>Digest</th><th>Size</th><th></th></tr></thead>
							<tbody>
								{#each data.project.files as file (file.external_file_id)}
									<tr>
										<td>{file.metadata.filename ?? file.external_file_id}</td>
										<td>{file.digest ? shortDigest(file.digest) : 'not observed'}</td>
										<td>{file.size ?? 'unknown'}</td>
										<td
											><a
												class="link"
												href={safeExternalUrl(file.source_url) ?? '#'}
												target="_blank"
												rel="noopener noreferrer">Source</a
											></td
										>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{/if}
			</div>
		</section>

		<p class="text-sm text-base-content/60">
			Observed {formatDate(data.project.observed_at)} by {data.project.bridge_id}
			{data.project.bridge_version}. Digests identify observed files; they do not grant
			redistribution rights or publisher authority.
		</p>
	{/if}
</div>
