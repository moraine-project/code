<script lang="ts">
	import { createExternalClaim } from '$lib/api/external';
	import { safeExternalUrl, shortDigest } from '$lib/api/registry';
	import { session } from '$lib/session.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let claimant = $state('');
	let challenge = $state('');
	let claimBusy = $state(false);
	let claimError = $state<string | null>(null);
	let claimNotice = $state<string | null>(null);

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

	async function submitClaim(event: SubmitEvent) {
		event.preventDefault();
		if (!data.project) return;
		claimBusy = true;
		claimError = null;
		claimNotice = null;
		try {
			const claim = await createExternalClaim(
				data.project.provider,
				data.project.external_project_id,
				claimant.trim(),
				challenge.trim(),
			);
			claimNotice = `Claim ${claim.id} recorded and awaits review.`;
			claimant = '';
			challenge = '';
		} catch (cause) {
			claimError = cause instanceof Error ? cause.message : 'could not record claim';
		} finally {
			claimBusy = false;
		}
	}
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

		{#if session.user?.role === 'operator'}
			<section class="card card-border bg-base-200">
				<div class="card-body gap-3">
					<h2 class="card-title">Record a native-project claim</h2>
					<p class="text-sm text-base-content/70">
						Record who claims this external project and where the directory can verify that claim. A
						claim does not rewrite the observed source or create a native project.
					</p>
					{#if claimError}<div role="alert" class="alert alert-error">
							<span>{claimError}</span>
						</div>{/if}
					{#if claimNotice}<div role="alert" class="alert alert-success">
							<span>{claimNotice}</span>
						</div>{/if}
					<form class="grid gap-3 sm:grid-cols-2" onsubmit={submitClaim}>
						<label class="floating-label">
							<span>Claimant reference</span>
							<input
								class="input w-full"
								bind:value={claimant}
								required
								aria-label="Claimant reference"
							/>
						</label>
						<label class="floating-label">
							<span>Challenge reference</span>
							<input
								class="input w-full"
								bind:value={challenge}
								required
								aria-label="Challenge reference"
							/>
						</label>
						<button class="btn btn-outline w-fit" type="submit" disabled={claimBusy}>
							{claimBusy ? 'Recording…' : 'Record claim'}
						</button>
					</form>
				</div>
			</section>
		{/if}

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
										<td>
											{#if safeExternalUrl(file.source_url)}
												<a
													class="link"
													href={safeExternalUrl(file.source_url) ?? undefined}
													target="_blank"
													rel="noopener noreferrer">Source</a
												>
											{:else}
												<span class="text-base-content/60">Unavailable</span>
											{/if}
										</td>
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
