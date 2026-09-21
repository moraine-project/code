<script lang="ts">
	import { onMount } from 'svelte';
	import { Plus, RefreshCw, RotateCcw, Unlink } from '@lucide/svelte';
	import {
		follow,
		pinMirror,
		resync,
		resetCursor,
		subscriptions,
		unfollow,
		witness,
		type WitnessReport,
		type Subscription,
	} from '$lib/api/federation';
	import Digest from '$lib/components/Digest.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';

	let homes = $state<Subscription[]>([]);
	let homeUrl = $state('');
	let projectId = $state('');
	let busy = $state(false);
	let error = $state<string | null>(null);
	let notice = $state<string | null>(null);
	let loading = $state(true);
	let witnessProjectId = $state('');
	let witnessReport = $state<WitnessReport | null>(null);
	let witnessBusy = $state(false);
	let mirrorId = $state('');
	let mirrorKey = $state('');

	onMount(load);

	async function load() {
		loading = true;
		try {
			homes = await subscriptions();
			error = null;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not load subscriptions';
		} finally {
			loading = false;
		}
	}

	async function startFollowing(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		try {
			await follow(homeUrl.trim(), projectId.trim());
			homeUrl = '';
			projectId = '';
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'the follow failed';
		} finally {
			busy = false;
		}
	}

	async function resyncNow() {
		busy = true;
		notice = null;
		try {
			const report = await resync();
			notice = `pulled ${report.synced} project(s); ${report.failed} failed. See the server log for failures.`;
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'the resync failed';
		} finally {
			busy = false;
		}
	}

	async function reset(home: Subscription) {
		busy = true;
		notice = null;
		try {
			await resetCursor(home.home_url, home.project_id);
			notice = 'cursor reset; the next sync re-reads the feed from the start.';
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not reset the cursor';
		} finally {
			busy = false;
		}
	}

	async function stopFollowing(home: Subscription) {
		busy = true;
		try {
			await unfollow(home.home_url, home.project_id);
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not unfollow';
		} finally {
			busy = false;
		}
	}

	async function inspectWitness(event: SubmitEvent) {
		event.preventDefault();
		witnessBusy = true;
		error = null;
		try {
			witnessReport = await witness(witnessProjectId.trim());
		} catch (cause) {
			witnessReport = null;
			error = cause instanceof Error ? cause.message : 'could not load witness observations';
		} finally {
			witnessBusy = false;
		}
	}

	async function registerMirror(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		error = null;
		notice = null;
		try {
			const id = await pinMirror(mirrorId.trim(), mirrorKey.trim());
			notice = `Mirror key pinned for ${id}.`;
			mirrorId = '';
			mirrorKey = '';
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not pin mirror key';
		} finally {
			busy = false;
		}
	}

	function formatTime(seconds: number): string {
		return new Date(seconds * 1000).toLocaleString();
	}
</script>

<div class="flex flex-col gap-4">
	<div class="flex flex-wrap items-center justify-between gap-2">
		<p class="text-sm text-base-content/70">
			Projects this instance pulls from other homes. Each home is verified against the project's own
			signed records, not trusted because of where it is.
		</p>
		<button class="btn btn-sm btn-outline" type="button" disabled={busy} onclick={resyncNow}>
			<RefreshCw size={15} />
			Pull all now
		</button>
	</div>

	{#if error}
		<div role="alert" class="alert alert-error"><span>{error}</span></div>
	{/if}
	{#if notice}
		<div role="alert" class="alert alert-info"><span>{notice}</span></div>
	{/if}

	{#if loading}
		<div class="skeleton h-16 w-full"></div>
	{:else if homes.length === 0}
		<EmptyState title="No homes followed" message="Add one below to pull a project." />
	{:else}
		<div class="overflow-x-auto">
			<table class="table table-sm">
				<thead>
					<tr>
						<th scope="col">Home</th>
						<th scope="col">Project</th>
						<th scope="col">Cursor</th>
						<th scope="col">Behind</th>
						<th scope="col">Last sync</th>
						<th scope="col"></th>
					</tr>
				</thead>
				<tbody>
					{#each homes as home (home.home_url + home.project_id)}
						<tr>
							<td class="break-all">{home.home_url}</td>
							<td><Digest value={home.project_id} label="the project id" length={12} /></td>
							<td>{home.cursor_seq}</td>
							<td>
								{#if home.lag_entries > 0}
									<span class="badge badge-warning badge-sm">{home.lag_entries}</span>
								{:else}
									<span class="text-base-content/50">0</span>
								{/if}
							</td>
							<td class="whitespace-nowrap">{formatTime(home.updated_at)}</td>
							<td>
								<div class="flex gap-1">
									<button class="btn btn-ghost btn-xs" disabled={busy} onclick={() => reset(home)}>
										<RotateCcw size={13} />
										reset
									</button>
									<button
										class="btn btn-ghost btn-xs"
										disabled={busy}
										onclick={() => stopFollowing(home)}
									>
										<Unlink size={13} />
										unfollow
									</button>
								</div>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}

	<form class="flex flex-wrap items-end gap-3" onsubmit={startFollowing}>
		<label class="floating-label">
			<span>Home URL</span>
			<input
				class="input w-64"
				type="url"
				bind:value={homeUrl}
				required
				placeholder="https://home.example"
			/>
		</label>
		<label class="floating-label">
			<span>Project ID</span>
			<input
				class="input w-72 font-mono text-xs"
				bind:value={projectId}
				required
				placeholder="gd:sha256:…"
			/>
		</label>
		<button class="btn" type="submit" disabled={busy}>
			<Plus size={16} />
			Pull project
		</button>
	</form>

	<section class="flex flex-col gap-3 border-t border-base-300 pt-5">
		<div>
			<h2 class="text-lg font-semibold">Witness log</h2>
			<p class="text-sm text-base-content/70">
				Compare the heads this instance and exchanged observers saw for one project. A conflict
				means observations disagree at the same feed sequence; it is not silently resolved here.
			</p>
		</div>
		<form class="flex flex-wrap items-end gap-3" onsubmit={inspectWitness}>
			<label class="floating-label">
				<span>Project ID</span>
				<input
					class="input w-72 font-mono text-xs"
					bind:value={witnessProjectId}
					required
					placeholder="gd:sha256:…"
					aria-label="Witness project ID"
				/>
			</label>
			<button class="btn" type="submit" disabled={witnessBusy}>
				{witnessBusy ? 'Loading…' : 'Inspect witness log'}
			</button>
		</form>
		{#if witnessReport}
			{#if witnessReport.conflicts.length > 0}
				<div role="alert" class="alert alert-warning">
					<span>{witnessReport.conflicts.length} conflicting feed head(s) need investigation.</span>
				</div>
			{:else}
				<p class="text-sm text-success">No conflicting heads were observed.</p>
			{/if}
			{#if witnessReport.observations.length === 0}
				<p class="text-sm text-base-content/60">No witness observations recorded.</p>
			{:else}
				<ul class="flex flex-col gap-2 text-sm">
					{#each witnessReport.observations as observation (observation.observer_id + observation.source_home + observation.sequence + observation.head_entry)}
						<li class="rounded-box border border-base-300 p-2">
							<div class="flex flex-wrap gap-x-3 gap-y-1">
								<span>sequence {observation.sequence}</span>
								<span class="text-base-content/60">observer {observation.observer_id}</span>
							</div>
							<div class="break-all text-xs text-base-content/60">{observation.source_home}</div>
							<div class="break-all font-mono text-xs">{observation.head_entry}</div>
						</li>
					{/each}
				</ul>
			{/if}
		{/if}
	</section>

	<section class="flex flex-col gap-3 border-t border-base-300 pt-5">
		<div>
			<h2 class="text-lg font-semibold">Mirror signing keys</h2>
			<p class="text-sm text-base-content/70">
				Pin a mirror's public key before accepting its signed holding commitments. The mirror signs
				its own commitments outside this console.
			</p>
		</div>
		<form class="grid gap-3 sm:grid-cols-2" onsubmit={registerMirror}>
			<label class="floating-label">
				<span>Mirror ID</span>
				<input
					class="input w-full"
					bind:value={mirrorId}
					required
					aria-label="Mirror ID"
					placeholder="archive.example"
				/>
			</label>
			<label class="floating-label">
				<span>Mirror public key</span>
				<input
					class="input w-full font-mono text-xs"
					bind:value={mirrorKey}
					required
					aria-label="Mirror public key"
					placeholder="64 hex characters"
				/>
			</label>
			<button class="btn w-fit" type="submit" disabled={busy}>Pin mirror key</button>
		</form>
	</section>
</div>
