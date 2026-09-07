<script lang="ts">
	import { goto } from '$app/navigation';
	import { untrack } from 'svelte';
	import { shortDigest } from '$lib/api/registry';
	import Digest from '$lib/components/Digest.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let loader = $state(untrack(() => data.loader));
	let loaderVersion = $state(untrack(() => data.loaderVersion));
	let gameVersion = $state(untrack(() => data.gameVersion));

	const loaders = $derived(
		Array.from(
			new Set(data.feed?.entries.flatMap((entry) => entry.release?.loaders ?? []) ?? []),
		).toSorted(),
	);

	function applyFilters() {
		const params = new URLSearchParams();
		if (data.home) params.set('home', data.home);
		if (gameVersion.trim()) params.set('game_version', gameVersion.trim());
		if (loader) params.set('loader', loader);
		if (loaderVersion.trim()) params.set('loader_version', loaderVersion.trim());
		goto(`/p/${encodeURIComponent(data.projectId)}?${params.toString()}`);
	}

	const entries = $derived(data.feed?.entries ?? []);
	const releases = $derived(entries.filter((entry) => entry.kind === 'release-published'));
	const releaseRows = $derived(
		entries.filter(
			(entry) => entry.kind === 'release-published' || entry.kind === 'release-withdrawn',
		),
	);
	const history = $derived(
		entries.filter(
			(entry) => entry.kind !== 'release-published' && entry.kind !== 'release-withdrawn',
		),
	);
	const latest = $derived(releases[0] ?? null);
	const gameId = $derived(
		entries.find((entry) => entry.release?.game_id)?.release?.game_id ?? null,
	);

	function formatTime(seconds: number): string {
		return new Date(seconds * 1000).toLocaleString();
	}

	function objectHex(id: string): string {
		return id.startsWith('gd:sha256:') ? id.slice('gd:sha256:'.length) : id;
	}

	const changeLabels: Record<string, string> = {
		'release-published': 'New release',
		'release-withdrawn': 'Withdrawn',
		'profile-updated': 'Description update',
		'key-changed': 'Signing key change',
		'ownership-transferred': 'New owner',
		migration: 'Moved home',
		recovery: 'Key recovery',
		advisory: 'Security note',
	};

	function changeLabel(kind: string): string {
		return changeLabels[kind] ?? kind.replaceAll('-', ' ');
	}

	function releaseHref(object: string): string {
		return `/p/${encodeURIComponent(data.projectId)}/release/${objectHex(object)}?home=${encodeURIComponent(data.home)}`;
	}
</script>

<svelte:head>
	<title>{data.profile?.display_name ?? data.projectId} · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<a class="link link-hover w-fit" href="/">← Open another project</a>

	{#if data.error}
		<div role="alert" class="alert alert-error alert-soft">
			<span>{data.error}</span>
		</div>
		<p class="text-base-content/70 text-sm">Home: <code>{data.home || '(none)'}</code></p>
	{:else if data.summary && data.feed}
		{#if data.summary.listing_state && data.summary.listing_state !== 'listed'}
			<div role="alert" class="alert alert-warning alert-soft">
				<span>
					This instance lists this project as <strong>{data.summary.listing_state}</strong>{data
						.summary.reason_code
						? ` (${data.summary.reason_code})`
						: ''}. That is a decision by this one instance, not a statement about the publisher or
					the files.
				</span>
			</div>
		{/if}

		<section class="card card-border bg-base-200">
			<div class="card-body gap-4">
				<div class="flex flex-wrap items-start justify-between gap-4">
					<div class="flex flex-col gap-2">
						<h1 class="text-2xl font-semibold tracking-tight">
							{data.profile?.display_name ?? shortDigest(data.summary.project_id, 24)}
						</h1>
						{#if data.profile?.summary}
							<p class="max-w-2xl text-base-content/80">{data.profile.summary}</p>
						{/if}
						<div class="flex flex-wrap gap-2">
							{#if gameId}
								<span class="badge badge-ghost" title={gameId}>Game {shortDigest(gameId, 16)}</span>
							{/if}
							<span class="badge badge-ghost">{data.summary.head_seq} updates</span>
							{#if data.profile}
								<span class="badge badge-outline">Profile published</span>
							{/if}
						</div>
					</div>
					{#if latest}
						<div class="card card-border w-full bg-base-100 sm:w-80">
							<div class="card-body gap-3">
								<span class="text-xs uppercase tracking-wide text-base-content/60">
									Latest release
								</span>
								<p class="text-lg font-semibold">{latest.title ?? 'untitled'}</p>
								<p class="text-sm text-base-content/70">
									{latest.release?.channel ?? 'release'}
									{#if latest.release?.loaders.length}
										· {latest.release.loaders.join(', ')}
									{/if}
									· {formatTime(latest.declared_at)}
								</p>
								{#if releases.length > 1}
									<label class="fieldset">
										<span class="label">Choose a version</span>
										<select
											class="select select-sm"
											aria-label="Choose a release version"
											onchange={(event) => goto(releaseHref(event.currentTarget.value))}
										>
											{#each releases as release (release.object)}
												<option value={release.object} selected={release.object === latest.object}>
													{release.title ?? 'untitled'} · {release.release?.channel ?? 'release'} · {formatTime(
														release.declared_at,
													)}
												</option>
											{/each}
										</select>
									</label>
								{/if}
								<a class="btn btn-primary btn-sm self-start" href={releaseHref(latest.object)}>
									View and download
								</a>
							</div>
						</div>
					{/if}
				</div>

				<details class="rounded-box border border-base-300 bg-base-100 p-3">
					<summary class="cursor-pointer text-sm font-medium">Technical details</summary>
					<dl class="mt-3 grid gap-2 text-sm sm:grid-cols-[9rem_1fr]">
						<dt class="text-base-content/60">Project ID</dt>
						<dd class="break-all font-mono">{data.summary.project_id}</dd>
						<dt class="text-base-content/60">Home</dt>
						<dd class="break-all">{data.home}</dd>
						<dt class="text-base-content/60">Genesis</dt>
						<dd class="break-all font-mono">{data.summary.genesis}</dd>
					</dl>
				</details>
			</div>
		</section>

		{#if data.profile}
			<section class="card card-border bg-base-200">
				<div class="card-body">
					<h2 class="card-title">About this project</h2>
					<p class="whitespace-pre-line text-base-content/80">{data.profile.description}</p>
					{#if data.profile.tags.length > 0 || data.profile.categories.length > 0}
						<div class="flex flex-wrap gap-2">
							{#each data.profile.categories as category (category)}
								<span class="badge badge-outline">{category}</span>
							{/each}
							{#each data.profile.tags as tag (tag)}
								<span class="badge badge-ghost">{tag}</span>
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

		<section class="card card-border bg-base-200">
			<div class="card-body gap-4">
				<div class="flex flex-col gap-1">
					<h2 class="card-title">Release history</h2>
					<p class="text-base-content/70 text-sm">
						Every published release, newest changes last. The home serves this list; it is signed by
						the publisher.
					</p>
				</div>

				<form
					class="flex flex-wrap items-end gap-3"
					onsubmit={(event) => {
						event.preventDefault();
						applyFilters();
					}}
				>
					<fieldset class="fieldset">
						<legend class="fieldset-legend">Game version</legend>
						<input
							class="input input-sm"
							bind:value={gameVersion}
							placeholder="1.20.1"
							aria-label="Filter by game version"
						/>
					</fieldset>
					{#if loaders.length > 0}
						<fieldset class="fieldset">
							<legend class="fieldset-legend">Loader</legend>
							<select class="select select-sm" bind:value={loader} aria-label="Filter by loader">
								<option value="">Any loader</option>
								{#each loaders as id (id)}
									<option value={id}>{id}</option>
								{/each}
							</select>
						</fieldset>
					{/if}
					<fieldset class="fieldset">
						<legend class="fieldset-legend">Loader version</legend>
						<input
							class="input input-sm"
							bind:value={loaderVersion}
							placeholder="0.15.0"
							aria-label="Filter by loader version"
						/>
					</fieldset>
					<button class="btn btn-sm" type="submit">Apply filters</button>
				</form>

				{#if data.feed.entries.length === 0}
					<p class="text-base-content/80 text-sm">This project has no releases yet.</p>
				{:else if releaseRows.length === 0}
					<p class="text-base-content/80 text-sm">No release matches these filters.</p>
				{:else}
					<div class="overflow-x-auto">
						<table class="table table-sm">
							<thead>
								<tr>
									<th scope="col">#</th>
									<th scope="col">Change</th>
									<th scope="col">Release</th>
									<th scope="col">Channel</th>
									<th scope="col">Loaders</th>
									<th scope="col">Published</th>
									<th scope="col"><span class="sr-only">Open</span></th>
								</tr>
							</thead>
							<tbody>
								{#each releaseRows as entry (entry.entry)}
									<tr>
										<td class="text-base-content/60">{entry.seq}</td>
										<td>{changeLabel(entry.kind)}</td>
										<td>
											{#if entry.kind === 'release-published' || entry.kind === 'release-withdrawn'}
												<a class="link link-hover" href={releaseHref(entry.object)}>
													{entry.title ?? '—'}
												</a>
											{:else}
												{entry.title ?? '—'}
											{/if}
										</td>
										<td>{entry.release?.channel ?? '—'}</td>
										<td>{entry.release?.loaders.join(', ') || '—'}</td>
										<td class="whitespace-nowrap text-base-content/70">
											{formatTime(entry.declared_at)}
										</td>
										<td>
											{#if entry.kind === 'release-published' || entry.kind === 'release-withdrawn'}
												<a class="btn btn-ghost btn-xs" href={releaseHref(entry.object)}>Open</a>
											{/if}
										</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
					{#if data.feed.truncated}
						<p class="text-base-content/60 text-sm" role="status">
							The home stopped scanning at its limit, so more matching entries may follow. Continue
							from sequence {data.feed.next}.
						</p>
					{/if}
				{/if}
			</div>
		</section>

		{#if history.length > 0}
			<section class="card card-border bg-base-200">
				<div class="card-body gap-3">
					<div class="flex flex-col gap-1">
						<h2 class="card-title">Project history</h2>
						<p class="text-base-content/70 text-sm">
							Changes to the project's identity and metadata, newest last: description updates,
							signing key changes, ownership transfers, and moves between homes. A changed publisher
							key is a security event, not a routine update.
						</p>
					</div>
					<ul class="flex flex-col gap-2">
						{#each history as entry (entry.entry)}
							<li class="flex flex-wrap items-center gap-2 text-sm">
								<span class="badge badge-outline">{changeLabel(entry.kind)}</span>
								<span class="text-base-content/70">{formatTime(entry.declared_at)}</span>
								<Digest copyOnly value={entry.entry} label="the history entry" />
							</li>
						{/each}
					</ul>
				</div>
			</section>
		{/if}

		<div role="alert" class="alert alert-info alert-soft">
			<span>
				This page fetched live data from <strong>{data.home}</strong> and checked that it reads correctly.
				It did not verify signatures. Use the verifier tool on the files you download to check the publisher's
				signature and the file fingerprint.
			</span>
		</div>
	{/if}

	<section class="text-sm text-base-content/60">
		<p>
			ID checks: <Digest copyOnly value={data.projectId} label="the project id" />
		</p>
	</section>
</div>
