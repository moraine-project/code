<script lang="ts">
	import { onMount } from 'svelte';
	import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';
	import {
		follow,
		resync,
		resetCursor,
		subscriptions,
		unfollow,
		type Subscription,
	} from '$lib/api/federation';
	import Digest from '$lib/components/Digest.svelte';

	let homes = $state<Subscription[]>([]);
	let error = $state<string | null>(null);
	let loading = $state(true);
	let homeUrl = $state('');
	let projectId = $state('');
	let busy = $state(false);
	let notice = $state<string | null>(null);

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

<svelte:head>
	<title>Advanced settings · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<h1 class="text-2xl font-bold">Advanced settings</h1>

	<section class="card card-border">
		<div class="card-body">
			<h2 class="card-title">Chosen directory</h2>
			<p class="text-base-content/80 text-sm">
				{#if PUBLIC_MORAINE_REGISTRY}
					This build resolves through <strong>{PUBLIC_MORAINE_REGISTRY}</strong> unless a page is given
					another home. A deployment can point elsewhere by setting that value at build time.
				{:else}
					This build has no default directory. Pages fall back to whatever home they are given.
				{/if}
			</p>
		</div>
	</section>

	<section class="card card-border">
		<div class="card-body">
			<div class="flex flex-wrap items-center justify-between gap-2">
				<h2 class="card-title">Direct homes</h2>
				<button class="btn btn-sm btn-outline" type="button" disabled={busy} onclick={resyncNow}>
					Pull all now
				</button>
			</div>
			<p class="text-base-content/80 text-sm">
				These are the projects this instance pulls from other homes. A home is addressed by its URL
				and verified against the project's own signed records, not trusted because of where it is.
			</p>
			{#if error}
				<div role="alert" class="alert alert-error"><span>{error}</span></div>
			{/if}
			{#if notice}
				<div role="alert" class="alert alert-success"><span>{notice}</span></div>
			{/if}
			{#if loading}
				<span class="loading loading-spinner loading-sm" aria-hidden="true"></span>
				<span class="sr-only" role="status">Loading direct homes</span>
			{:else if homes.length === 0}
				<p class="text-base-content/60 text-sm">No direct homes are followed yet.</p>
			{:else}
				<div class="overflow-x-auto">
					<table class="table table-sm">
						<thead>
							<tr>
								<th scope="col">Home</th>
								<th scope="col">Project</th>
								<th scope="col">Cursor</th>
								<th scope="col">Behind</th>
								<th scope="col">Resets</th>
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
											<span class="text-base-content/60">0</span>
										{/if}
									</td>
									<td>
										{#if home.resets > 0}
											<span class="badge badge-ghost badge-sm">{home.resets}</span>
										{:else}
											<span class="text-base-content/60">0</span>
										{/if}
									</td>
									<td>{formatTime(home.updated_at)}</td>
									<td>
										<div class="flex gap-1">
											<button
												class="btn btn-ghost btn-xs"
												type="button"
												disabled={busy}
												onclick={() => reset(home)}
												aria-label={`Reset the cursor for ${home.project_id} from ${home.home_url}`}
											>
												reset
											</button>
											<button
												class="btn btn-ghost btn-xs"
												type="button"
												disabled={busy}
												onclick={() => stopFollowing(home)}
												aria-label={`Stop pulling ${home.project_id} from ${home.home_url}`}
											>
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
				<fieldset class="fieldset">
					<legend class="fieldset-legend">Home URL</legend>
					<input
						class="input"
						type="url"
						bind:value={homeUrl}
						required
						placeholder="https://home.example"
						aria-label="Home URL"
					/>
				</fieldset>
				<fieldset class="fieldset">
					<legend class="fieldset-legend">Project ID</legend>
					<input
						class="input font-mono"
						bind:value={projectId}
						required
						placeholder="gd:sha256:…"
						aria-label="Project ID"
					/>
				</fieldset>
				<button class="btn" type="submit" disabled={busy}>Pull project</button>
			</form>
			<p class="text-base-content/60 text-sm">
				Following fetches and verifies the project's genesis and feed from that home. It does not
				grant the home any authority over your copy.
			</p>
		</div>
	</section>
</div>
