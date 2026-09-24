<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { Download } from '@lucide/svelte';
	import Avatar from '$lib/components/Avatar.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import SelectField, { type SelectOption } from '$lib/components/SelectField.svelte';
	import { safeExternalUrl } from '$lib/api/external-url';
	import { shortDigest } from '$lib/api/digests';
	import type { FeedEntry } from '$lib/api/projects';
	import { follow, follows, unfollow } from '$lib/api/notifications';
	import { session } from '$lib/session.svelte';
	import { home } from '$lib/home.svelte';
	import { pageTitle } from '$lib/title.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	let tab = $state<'description' | 'versions'>('description');
	let following = $state(false);
	let followBusy = $state(false);
	let followError = $state<string | null>(null);

	$effect(() => {
		if (!session.user) {
			following = false;
			return;
		}
		void follows()
			.then((projects) => (following = projects.includes(data.projectId)))
			.catch(() => (following = false));
	});

	async function toggleFollow() {
		if (!session.user) {
			goto('/account');
			return;
		}
		if (!session.canWrite) {
			followError = session.writeBlockedReason;
			return;
		}
		followBusy = true;
		followError = null;
		try {
			if (following) await unfollow(data.projectId);
			else await follow(data.projectId);
			following = !following;
		} catch (cause) {
			followError = cause instanceof Error ? cause.message : 'could not update following';
		} finally {
			followBusy = false;
		}
	}

	function hexOf(id: string): string {
		return id.replace(/^gd:sha256:/, '');
	}

	function releaseUrl(entry: FeedEntry): string {
		return home.url(`/p/${encodeURIComponent(data.projectId)}/release/${hexOf(entry.object)}`);
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

	const releases = $derived(
		(data.feed?.entries ?? []).filter((entry) => entry.release).toReversed(),
	);
	const packs = $derived(
		(data.feed?.entries ?? []).filter((entry) => entry.kind === 'modpack-published').toReversed(),
	);

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
		const params = new URLSearchParams(page.url.searchParams);
		for (const [key, value] of Object.entries(updates)) {
			if (value) {
				params.set(key, value);
			} else {
				params.delete(key);
			}
		}
		const query = params.toString();
		goto(home.url(`/p/${encodeURIComponent(data.projectId)}${query ? `?${query}` : ''}`), {
			keepFocus: true,
			noScroll: true,
		});
	}
</script>

<svelte:head>
	<title>{pageTitle(data.profile?.display_name ?? data.projectId)}</title>
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
			<button
				class="btn btn-outline"
				type="button"
				onclick={toggleFollow}
				disabled={followBusy || !session.canWrite}
			>
				{followBusy
					? 'Saving…'
					: following
						? 'Unfollow'
						: session.user
							? 'Follow'
							: 'Sign in to follow'}
			</button>
			{#if session.writeBlockedReason}
				<p class="max-w-xs text-xs text-base-content/60">{session.writeBlockedReason}</p>
			{/if}
			{#if followError}<p role="alert" class="max-w-xs text-xs text-error">{followError}</p>{/if}
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
						{#each links as link (link.url)}
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

			<section class="rounded-box border border-base-300 bg-base-200 p-4">
				<h2 class="text-sm font-semibold">Provenance</h2>
				<dl class="mt-2 grid gap-2 text-sm sm:grid-cols-2">
					<div>
						<dt class="text-xs text-base-content/60">Home</dt>
						<dd class="break-all font-mono">{data.home}</dd>
					</div>
					<div>
						<dt class="text-xs text-base-content/60">Feed head</dt>
						<dd>{data.summary?.head_seq ?? 0} signed entries</dd>
					</div>
					<div>
						<dt class="text-xs text-base-content/60">Project id</dt>
						<dd><span class="font-mono">{shortDigest(data.projectId, 20)}</span></dd>
					</div>
					<div>
						<dt class="text-xs text-base-content/60">Profile revision</dt>
						<dd>
							<span class="font-mono"
								>{data.profile?.revision
									? shortDigest(data.profile.revision, 16)
									: 'not published'}</span
							>
						</dd>
					</div>
				</dl>
				<p class="mt-3 text-xs text-base-content/60">
					The home controls its signed feed. Directory listing and review decisions are local to
					this instance.
				</p>
				{#if data.denyEntries.length > 0}
					<div role="alert" class="alert alert-warning mt-4">
						<div>
							<p class="font-semibold">External policy annotations</p>
							<p class="text-xs">
								{data.denyEntries.length} subscribed deny-list annotation(s) apply to this project. They
								are policy signals attributed to their issuers, not proof that signed records are invalid.
							</p>
							<ul class="mt-2 flex flex-col gap-1 text-xs">
								{#each data.denyEntries as entry (entry.deny_list + entry.reason_code)}
									<li>{entry.reason_code} · {entry.issuer_id} · scope {entry.scope_kind}</li>
								{/each}
							</ul>
						</div>
					</div>
				{/if}
				{#if data.channels.length > 0}
					<div class="mt-4">
						<h3 class="text-sm font-semibold">Current channels</h3>
						<div class="mt-2 flex flex-wrap gap-2">
							{#each data.channels as channel}
								<a
									class="badge badge-outline"
									href={releaseUrl({ object: channel.release } as FeedEntry)}
									>{channel.channel} · {channel.human_version}</a
								>
							{/each}
						</div>
					</div>
				{/if}
				{#if data.recovery}
					<div class="mt-4 text-sm">
						<h3 class="font-semibold">Signing roots</h3>
						<p class="text-xs text-base-content/60">
							Threshold {data.recovery.threshold}; {data.recovery.recovered
								? `recovered at feed sequence ${data.recovery.valid_from_seq}`
								: 'original project roots'}.
						</p>
					</div>
				{/if}
				{#if data.migrations.length > 0}
					<div class="mt-4 text-sm">
						<h3 class="font-semibold">Home history</h3>
						<ul class="mt-2 flex flex-col gap-1 text-xs text-base-content/70">
							{#each data.migrations as migration}
								<li>
									{migration.old_home} → {migration.new_home} at feed sequence {migration.cutover_seq}
								</li>
							{/each}
						</ul>
					</div>
				{/if}
			</section>
		</section>
	{:else}
		<section class="flex flex-col gap-4">
			{#if packs.length > 0}
				<div class="flex flex-col gap-2">
					<h2 class="text-sm font-semibold text-base-content/60">Modpack manifests</h2>
					{#each packs as entry (entry.entry)}
						<a
							class="link link-hover font-mono text-sm"
							href={home.url(`/packs/${hexOf(entry.object)}`)}
						>
							{shortDigest(entry.object)} · {formatDate(entry.declared_at)}
						</a>
					{/each}
				</div>
			{/if}
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
