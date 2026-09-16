<script lang="ts">
	import { onMount } from 'svelte';
	import { Plus, RefreshCw, RotateCcw, Unlink } from '@lucide/svelte';
	import {
		follow,
		resync,
		resetCursor,
		subscriptions,
		unfollow,
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
</div>
