<script lang="ts">
	import { onMount } from 'svelte';
	import { DownloadCloud } from '@lucide/svelte';
	import {
		definitionSubscriptions,
		subscribeDefinition,
		syncDefinition,
		type DefinitionSubscription,
	} from '$lib/api/federation';
	import {
		definitionRouteSegment,
		listDefinitions,
		type DefinitionSummary,
	} from '$lib/api/definitions';
	import { shortDigest } from '$lib/api/digests';
	import { apiOrigin } from '$lib/api/session';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import SelectField, { type SelectOption } from '$lib/components/SelectField.svelte';
	import SourceBadge from '$lib/components/SourceBadge.svelte';

	let definitions = $state<DefinitionSummary[]>([]);
	let subscriptions = $state<DefinitionSubscription[]>([]);
	let kind = $state<'game' | 'loader' | 'runtime'>('game');
	let id = $state('');
	let home = $state('');
	let busy = $state(false);
	let error = $state<string | null>(null);
	let notice = $state<string | null>(null);

	const kindOptions: SelectOption[] = [
		{ value: 'game', label: 'Game' },
		{ value: 'loader', label: 'Loader' },
		{ value: 'runtime', label: 'Runtime' },
	];

	onMount(load);

	async function load() {
		const base = apiOrigin();
		const [games, loaders, runtimes, definitionFeeds] = await Promise.all([
			listDefinitions(base, 'games').catch(() => []),
			listDefinitions(base, 'loaders').catch(() => []),
			listDefinitions(base, 'runtimes').catch(() => []),
			definitionSubscriptions().catch(() => []),
		]);
		definitions = [...games, ...loaders, ...runtimes];
		subscriptions = definitionFeeds;
	}

	async function subscribe() {
		if (!home.trim() || !id.trim()) {
			error = 'enter a home URL and definition ID first';
			return;
		}
		busy = true;
		error = null;
		notice = null;
		try {
			const pulled = await subscribeDefinition(home.trim(), id.trim(), kind);
			notice = `Subscribed to ${pulled.kind} ${shortDigest(pulled.id, 16)}.`;
			id = '';
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'the subscription failed';
		} finally {
			busy = false;
		}
	}

	async function pull(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		error = null;
		notice = null;
		try {
			const pulled = await syncDefinition(home.trim(), id.trim(), kind);
			notice = `Pulled ${pulled.kind} ${shortDigest(pulled.id, 16)}.`;
			id = '';
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'the pull failed';
		} finally {
			busy = false;
		}
	}
</script>

<div class="flex flex-col gap-4">
	{#if error}
		<div role="alert" class="alert alert-error"><span>{error}</span></div>
	{/if}
	{#if notice}
		<div role="alert" class="alert alert-info"><span>{notice}</span></div>
	{/if}

	<section class="flex flex-col gap-2 border-b border-base-300 pb-4">
		<h2 class="text-lg font-semibold">Definition subscriptions</h2>
		<p class="text-sm text-base-content/70">
			Subscribed definitions refresh during maintenance and keep their signed identity and source
			home.
		</p>
		{#if subscriptions.length === 0}
			<p class="text-sm text-base-content/60">No definition subscriptions yet.</p>
		{:else}
			<ul class="flex flex-col gap-1 text-sm">
				{#each subscriptions as subscription (subscription.home_url + subscription.id)}
					<li class="flex flex-wrap gap-x-2 gap-y-1">
						<span class="badge badge-ghost badge-sm">{subscription.kind}</span>
						<span class="font-mono text-xs">{shortDigest(subscription.id, 16)}</span>
						<span class="break-all text-base-content/60">from {subscription.home_url}</span>
					</li>
				{/each}
			</ul>
		{/if}
	</section>

	<p class="text-sm text-base-content/70">
		Games, loaders, and runtimes this instance knows. A definition is either authored here (<span
			class="font-medium">Local</span
		>) or pulled from another home (<span class="font-medium">Federated</span>); both keep the same
		ID wherever they are served. Import a whole set from the command line with
		<code>moraine-publish define --dir definitions/minecraft --out data/definitions --home …</code>.
	</p>

	{#if definitions.length === 0}
		<EmptyState title="No definitions yet" message="Author one from a file, or pull one below." />
	{:else}
		<div class="overflow-x-auto">
			<table class="table table-sm">
				<thead>
					<tr>
						<th scope="col">Kind</th>
						<th scope="col">Name</th>
						<th scope="col">ID</th>
						<th scope="col">Source</th>
					</tr>
				</thead>
				<tbody>
					{#each definitions as definition (definition.id)}
						{@const segment = definitionRouteSegment(definition.kind)}
						<tr>
							<td><span class="badge badge-ghost badge-sm">{definition.kind}</span></td>
							<td class="truncate">{definition.display_name ?? '—'}</td>
							<td>
								{#if segment}
									<a
										class="link link-hover font-mono text-xs"
										href={`/definitions/${segment}/${encodeURIComponent(definition.id)}`}
									>
										{shortDigest(definition.id, 16)}
									</a>
								{:else}
									<span class="font-mono text-xs">{shortDigest(definition.id, 16)}</span>
								{/if}
							</td>
							<td><SourceBadge sourceHome={definition.source_home} /></td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}

	<form class="flex flex-wrap items-end gap-3" onsubmit={pull}>
		<div class="w-36">
			<SelectField label="Kind" bind:value={kind} options={kindOptions} />
		</div>
		<label class="floating-label">
			<span>Definition ID</span>
			<input
				class="input w-72 font-mono text-xs"
				bind:value={id}
				required
				placeholder="gd:sha256:…"
			/>
		</label>
		<label class="floating-label">
			<span>From home</span>
			<input
				class="input w-64"
				type="url"
				bind:value={home}
				required
				placeholder="https://home.example"
			/>
		</label>
		<button class="btn" type="submit" disabled={busy}>
			<DownloadCloud size={16} />
			Pull definition
		</button>
		<button class="btn btn-outline" type="button" disabled={busy} onclick={subscribe}>
			Subscribe and pull
		</button>
	</form>
</div>
