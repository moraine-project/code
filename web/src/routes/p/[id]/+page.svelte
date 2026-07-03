<script lang="ts">
	import { shortDigest } from '$lib/api/registry';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	function formatTime(seconds: number): string {
		return new Date(seconds * 1000).toLocaleString();
	}
</script>

<svelte:head>
	<title>{data.projectId} · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<a class="link link-hover w-fit" href="/">← Resolve another project</a>

	{#if data.error}
		<div role="alert" class="alert alert-error">
			<span>{data.error}</span>
		</div>
		<p class="text-base-content/80 text-sm">Home: <code>{data.home || '(none)'}</code></p>
	{:else if data.summary && data.feed}
		<section class="card card-border">
			<div class="card-body">
				<h1 class="card-title break-all">{shortDigest(data.summary.project_id, 24)}</h1>
				<div class="flex flex-wrap gap-2">
					<span class="badge badge-outline">head #{data.summary.head_seq}</span>
					{#if data.summary.profile}
						<span class="badge badge-outline">profile published</span>
					{/if}
				</div>
				<dl class="grid gap-2 text-sm sm:grid-cols-[10rem_1fr]">
					<dt class="text-base-content/60">Project ID</dt>
					<dd class="break-all font-mono">{data.summary.project_id}</dd>
					<dt class="text-base-content/60">Home</dt>
					<dd class="break-all">{data.home}</dd>
					<dt class="text-base-content/60">Genesis</dt>
					<dd class="break-all font-mono">{data.summary.genesis}</dd>
				</dl>
			</div>
		</section>

		<section class="card card-border">
			<div class="card-body">
				<h2 class="card-title">Feed</h2>
				{#if data.feed.entries.length === 0}
					<p class="text-base-content/80 text-sm">No feed entries yet.</p>
				{:else}
					<div class="overflow-x-auto">
						<table class="table table-sm">
							<thead>
								<tr>
									<th scope="col">Seq</th>
									<th scope="col">Kind</th>
									<th scope="col">Object</th>
									<th scope="col">Declared</th>
								</tr>
							</thead>
							<tbody>
								{#each data.feed.entries as entry (entry.entry)}
									<tr>
										<td>{entry.seq}</td>
										<td>{entry.kind}</td>
										<td class="font-mono">{shortDigest(entry.object)}</td>
										<td>{formatTime(entry.declared_at)}</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{/if}
			</div>
		</section>

		<div role="alert" class="alert alert-info">
			<span>
				This page fetched live data from <strong>{data.home}</strong> and checked that it parses.
				It did not verify signatures. Run the verifier CLI against the files you download to check
				the publisher's signature and the artifact digest.
			</span>
		</div>
	{/if}
</div>
