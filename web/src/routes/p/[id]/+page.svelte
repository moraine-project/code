<script lang="ts">
	import { shortDigest } from '$lib/api/registry';
	import Digest from '$lib/components/Digest.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	function formatTime(seconds: number): string {
		return new Date(seconds * 1000).toLocaleString();
	}

	function objectHex(id: string): string {
		return id.startsWith('gd:sha256:') ? id.slice('gd:sha256:'.length) : id;
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
				<h1 class="card-title break-all">
					{data.profile?.display_name ?? shortDigest(data.summary.project_id, 24)}
				</h1>
				{#if data.profile}
					<p class="text-base-content/80">{data.profile.summary}</p>
				{/if}
				<div class="flex flex-wrap gap-2">
					<span class="badge badge-outline">head #{data.summary.head_seq}</span>
					{#if data.profile}
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

		{#if data.profile}
			<section class="card card-border">
				<div class="card-body">
					<h2 class="card-title">About</h2>
					<p class="whitespace-pre-line text-base-content/80">{data.profile.description}</p>
					{#if data.profile.tags.length > 0 || data.profile.categories.length > 0}
						<div class="flex flex-wrap gap-2">
							{#each data.profile.categories as category (category)}
								<span class="badge badge-outline">{category}</span>
							{/each}
							{#each data.profile.tags as tag (tag)}
								<span class="badge">{tag}</span>
							{/each}
						</div>
					{/if}
					{#if data.profile.links.length > 0 || data.profile.communities.length > 0}
						<ul class="flex flex-col gap-1 text-sm">
							{#each [...data.profile.links, ...data.profile.communities] as link (link.url)}
								<li>
									<span class="text-base-content/60">{link.kind}</span>
									<a
										class="link link-hover break-all"
										href={link.url}
										rel="noreferrer noopener"
										target="_blank"
									>
										{link.url}
									</a>
								</li>
							{/each}
						</ul>
					{/if}
				</div>
			</section>
		{/if}

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
									<th scope="col">Title</th>
									<th scope="col">Object</th>
									<th scope="col">Declared</th>
								</tr>
							</thead>
							<tbody>
								{#each data.feed.entries as entry (entry.entry)}
									<tr>
										<td>{entry.seq}</td>
										<td>{entry.kind}</td>
										<td>
											{#if entry.kind === 'release-published' || entry.kind === 'release-withdrawn'}
												<a
													class="link link-hover"
													href={`/p/${encodeURIComponent(data.projectId)}/release/${objectHex(entry.object)}?home=${encodeURIComponent(data.home)}`}
												>
													{entry.title ?? '—'}
												</a>
											{:else}
												{entry.title ?? '—'}
											{/if}
										</td>
										<td><Digest value={entry.object} label="the object id" length={12} /></td>
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
				This page fetched live data from <strong>{data.home}</strong> and checked that it parses. It did
				not verify signatures. Run the verifier CLI against the files you download to check the publisher's
				signature and the artifact digest.
			</span>
		</div>
	{/if}
</div>
