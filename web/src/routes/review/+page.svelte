<script lang="ts">
	import { onMount } from 'svelte';
	import { shortDigest } from '$lib/api/registry';
	import Digest from '$lib/components/Digest.svelte';
	import { assign, decide, reasonCodes, reviewQueue, type Submission } from '$lib/api/review';

	let submissions = $state<Submission[]>([]);
	let reasons = $state<Record<string, string>>({});
	let error = $state<string | null>(null);
	let notice = $state<string | null>(null);
	let loading = $state(true);
	let busy = $state<string | null>(null);

	onMount(load);

	async function load() {
		loading = true;
		try {
			submissions = await reviewQueue();
			for (const submission of submissions) {
				reasons[submission.id] ??= reasonCodes[0];
			}
			error = null;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not load the queue';
		} finally {
			loading = false;
		}
	}

	async function act(submission: Submission, decision: 'accept' | 'reject' | 'quarantine') {
		busy = submission.id;
		notice = null;
		error = null;
		try {
			const reason = decision === 'accept' ? undefined : reasons[submission.id];
			await decide(submission.id, decision, reason);
			notice = `${decision} recorded for ${shortDigest(submission.object)}.`;
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'the decision failed';
		} finally {
			busy = null;
		}
	}

	async function take(submission: Submission) {
		busy = submission.id;
		notice = null;
		error = null;
		try {
			await assign(submission.id);
			notice = `assigned ${shortDigest(submission.object)} to you.`;
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'the assignment failed';
		} finally {
			busy = null;
		}
	}

	function formatTime(seconds: number): string {
		return new Date(seconds * 1000).toLocaleString();
	}
</script>

<svelte:head>
	<title>Review queue · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<div class="flex flex-wrap items-center justify-between gap-2">
		<h1 class="text-2xl font-bold">Review queue</h1>
		<a class="btn btn-ghost" href="/account">Account</a>
	</div>
	<p class="text-base-content/80 max-w-2xl">
		A decision applies to one release digest and never alters signed bytes. Accepting commits the
		entry to this home's feed; rejecting or quarantining records the decision and leaves the signed
		record stored for audit.
	</p>

	{#if error}
		<div role="alert" class="alert alert-error"><span>{error}</span></div>
	{/if}
	{#if notice}
		<div role="alert" class="alert alert-success"><span>{notice}</span></div>
	{/if}

	{#if loading}
		<span class="loading loading-spinner loading-md"></span>
	{:else if submissions.length === 0}
		<p class="text-base-content/60 text-sm">Nothing is waiting for review.</p>
	{:else}
		<div class="overflow-x-auto">
			<table class="table">
				<thead>
					<tr>
						<th scope="col">Object</th>
						<th scope="col">Project</th>
						<th scope="col">Submitted</th>
						<th scope="col">State</th>
						<th scope="col">Reason</th>
						<th scope="col"></th>
					</tr>
				</thead>
				<tbody>
					{#each submissions as submission (submission.id)}
						<tr>
							<td><Digest value={submission.object} label="the object id" /></td>
							<td><Digest value={submission.project_id} label="the project id" /></td>
							<td>{formatTime(submission.created_at)}</td>
							<td>
								<span class="badge badge-outline">{submission.state}</span>
								{#if submission.assigned_to}
									<span class="text-base-content/60 text-xs">yours</span>
								{/if}
							</td>
							<td>
								<select
									class="select select-sm"
									bind:value={reasons[submission.id]}
									aria-label="Reason code"
								>
									{#each reasonCodes as code (code)}
										<option value={code}>{code}</option>
									{/each}
								</select>
							</td>
							<td>
								<div class="flex flex-wrap gap-1">
									{#if submission.state === 'submitted'}
										<button
											class="btn btn-sm btn-ghost"
											onclick={() => take(submission)}
											disabled={busy === submission.id}
										>
											Take
										</button>
									{/if}
									<button
										class="btn btn-sm"
										onclick={() => act(submission, 'accept')}
										disabled={busy === submission.id}
									>
										Accept
									</button>
									<button
										class="btn btn-sm btn-outline"
										onclick={() => act(submission, 'reject')}
										disabled={busy === submission.id}
									>
										Reject
									</button>
									<button
										class="btn btn-sm btn-outline"
										onclick={() => act(submission, 'quarantine')}
										disabled={busy === submission.id}
									>
										Quarantine
									</button>
								</div>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</div>
