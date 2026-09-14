<script lang="ts">
	import { goto } from '$app/navigation';
	import { Download } from '@lucide/svelte';
	import Avatar from '$lib/components/Avatar.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import SelectField, { type SelectOption } from '$lib/components/SelectField.svelte';
	import { safeExternalUrl, shortDigest, type FeedEntry } from '$lib/api/registry';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	let tab = $state<'description' | 'versions'>('description');

	function hexOf(id: string): string {
		return id.replace(/^gd:sha256:/, '');
	}

	function releaseUrl(entry: FeedEntry): string {
		return `/p/${encodeURIComponent(data.projectId)}/release/${hexOf(entry.object)}`;
	}

	function formatDate(seconds: number): string {
		return new Date(seconds * 1000).toLocaleDateString(undefined, {
			year: 'numeric',
			month: 'short',
			day: 'numeric',
		});
	}

	const loaderNames = $derived(
		new Map(
			data.loaders.map((loader) => [loader.id, loader.display_name ?? shortDigest(loader.id)]),
		),
	);

	const gameName = $derived(data.gamePayload?.display_name ?? '');

	const releases = $derived((data.feed?.entries ?? []).toReversed());

	const links = $derived(
		[...(data.profile?.links ?? []), ...(data.profile?.communities ?? [])]
			.map((link) => ({ kind: link.kind, url: safeExternalUrl(link.url) }))
			.filter((link): link is { kind: string; url: string } => link.url !== null),
	);

	const latest = $derived(releases.find((entry) => entry.release) ?? null);

	const versionOptions = $derived<SelectOption[]>([
		{ value: '', label: 'Any version' },
		...(data.gamePayload?.version_catalog ?? []).map((version) => ({
			value: version,
			label: version,
		})),
	]);

	const loaderOptions = $derived<SelectOption[]>([
		{ value: '', label: 'Any loader' },
		...data.loaders.map((loader) => ({
			value: loader.id,
			label: loader.display_name ?? shortDigest(loader.id),
		})),
	]);

	function apply(updates: Record<string, string>) {
		const params = new URLSearchParams();
		for (const [key, value] of Object.entries(updates)) {
			if (value) {
				params.set(key, value);
			}
		}
		const query = params.toString();
		goto(`/p/${encodeURIComponent(data.projectId)}${query ? `?${query}` : ''}`, {
			keepFocus: true,
			noScroll: true,
		});
	}
</script>

<svelte:head>
	<title>{data.profile?.display_name ?? data.projectId} · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	{#if data.error}
		<div role="alert" class="alert alert-error"><span>{data.error}</span></div>
	{/if}

	{#if data.summary?.listing_state && data.summary.listing_state !== 'listed'}
		<div role="alert" class="alert alert-warning">
			<span>This project is {data.summary.listing_state} on this instance.</span>
		</div>
	{/if}

	<header
		class="flex flex-col gap-5 rounded-box border border-base-300 bg-base-200 p-5 sm:flex-row sm:p-6"
	>
		<Avatar name={data.profile?.display_name ?? data.projectId} id={data.projectId} size={96} />
		<div class="flex min-w-0 flex-1 flex-col gap-3">
			<div class="flex flex-col gap-1">
				<h1 class="text-2xl font-bold tracking-tight sm:text-3xl">
					{data.profile?.display_name ?? 'Unnamed project'}
				</h1>
				{#if data.profile?.summary}
					<p class="text-base-content/70">{data.profile.summary}</p>
				{/if}
			</div>

			<div class="flex flex-wrap items-center gap-2">
				{#if gameName}
					<span class="badge badge-ghost">{gameName}</span>
				{/if}
				{#each data.profile?.categories ?? [] as category (category)}
					<span class="badge badge-outline">{category}</span>
				{/each}
				{#each data.profile?.tags ?? [] as tag (tag)}
					<span class="badge badge-soft">{tag}</span>
				{/each}
			</div>

			<div class="flex flex-wrap items-center gap-3 text-xs text-base-content/50">
				<span class="font-mono">{shortDigest(data.projectId, 20)}</span>
				{#if data.summary}
					<span>·</span>
					<span>{data.summary.head_seq} feed entries</span>
				{/if}
			</div>
		</div>

		<div class="flex shrink-0 flex-col items-stretch gap-2 sm:items-end">
			{#if latest}
				<a class="btn btn-primary" href={releaseUrl(latest)}>
					<Download size={16} />
					Download
				</a>
				<p class="text-xs text-base-content/50">
					{latest.title ?? 'Latest release'} · {formatDate(latest.declared_at)}
				</p>
			{:else}
				<span class="badge badge-ghost">No releases yet</span>
			{/if}
		</div>
	</header>

	<div role="tablist" class="tabs tabs-border">
		<button
			role="tab"
			class="tab"
			class:tab-active={tab === 'description'}
			onclick={() => (tab = 'description')}
		>
			Description
		</button>
		<button
			role="tab"
			class="tab"
			class:tab-active={tab === 'versions'}
			onclick={() => (tab = 'versions')}
		>
			Versions
			{#if releases.length > 0}
				<span class="badge badge-sm badge-ghost">{releases.length}</span>
			{/if}
		</button>
	</div>

	{#if tab === 'description'}
		<section class="flex flex-col gap-6">
			{#if data.profile?.description}
				<div class="prose-sm max-w-3xl whitespace-pre-line leading-relaxed">
					{data.profile.description}
				</div>
			{:else}
				<p class="text-base-content/60">No description published.</p>
			{/if}

			{#if (data.profile?.links?.length ?? 0) > 0 || (data.profile?.communities?.length ?? 0) > 0}
				<div class="flex flex-col gap-2">
					<h2 class="text-sm font-semibold text-base-content/60">Links</h2>
					<div class="flex flex-wrap gap-2">
						{#each [...(data.profile?.links ?? []), ...(data.profile?.communities ?? [])] as link (link.url)}
							<a
								class="btn btn-sm btn-outline"
								href={link.url}
								rel="noopener noreferrer"
								target="_blank"
							>
								{link.kind}
							</a>
						{/each}
					</div>
				</div>
			{/if}
		</section>
	{:else}
		<section class="flex flex-col gap-4">
			{#if data.gameId}
				<div class="grid gap-3 sm:grid-cols-2 lg:max-w-xl">
					<SelectField
						label="Game version"
						value={data.gameVersion}
						options={versionOptions}
						placeholder="Any version"
						onchange={(value) =>
							apply({ game_version: value, loader: data.loader, loader_version: '' })}
					/>
					<SelectField
						label="Loader"
						value={data.loader}
						options={loaderOptions}
						placeholder="Any loader"
						onchange={(value) =>
							apply({ game_version: data.gameVersion, loader: value, loader_version: '' })}
					/>
				</div>
			{/if}

			{#if releases.length === 0}
				<EmptyState title="No versions match" message="Try removing a filter." />
			{:else}
				<div class="flex flex-col gap-2">
					{#each releases as entry (entry.entry)}
						<div
							class="flex flex-wrap items-center gap-3 rounded-box border border-base-300 bg-base-200 p-3"
						>
							<div class="min-w-0 flex-1">
								<div class="flex flex-wrap items-center gap-2">
									<p class="font-medium">{entry.title ?? shortDigest(entry.object)}</p>
									{#if entry.release?.channel}
										<span class="badge badge-sm badge-ghost">{entry.release.channel}</span>
									{/if}
								</div>
								<div class="mt-1 flex flex-wrap items-center gap-2 text-xs text-base-content/60">
									{#each entry.release?.loaders ?? [] as loader (loader)}
										<span class="badge badge-xs badge-outline">
											{loaderNames.get(loader) ?? shortDigest(loader, 8)}
										</span>
									{/each}
									<span>{formatDate(entry.declared_at)}</span>
								</div>
							</div>
							<a class="btn btn-sm" href={releaseUrl(entry)}>View</a>
						</div>
					{/each}
				</div>
			{/if}
		</section>
	{/if}
</div>
