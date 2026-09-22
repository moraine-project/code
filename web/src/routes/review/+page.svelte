<script lang="ts">
	import { onMount } from 'svelte';
	import { Check, ShieldAlert, UserCheck, X } from '@lucide/svelte';
	import { shortDigest } from '$lib/api/registry';
	import Digest from '$lib/components/Digest.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import PageHeader from '$lib/components/PageHeader.svelte';
	import SelectField from '$lib/components/SelectField.svelte';
	import { assign, decide, reasonCodes, reviewQueue, type Submission } from '$lib/api/review';
	import { session } from '$lib/session.svelte';

	const pageSize = 100;

	let submissions = $state<Submission[]>([]);
	let reasons = $state<Record<string, string>>({});
	let error = $state<string | null>(null);
	let notice = $state<string | null>(null);
	let loading = $state(true);
	let busy = $state<string | null>(null);
	let more = $state(false);
	let hasMore = $state(false);
	let me = $state<string | null>(null);

	onMount(load);

	async function load() {
		loading = true;
		me ??= (await session.refresh())?.user_id ?? null;
		try {
			submissions = await reviewQueue(pageSize);
			hasMore = submissions.length === pageSize;
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

	async function loadMore() {
		more = true;
		notice = null;
		error = null;
		try {
			const last = submissions.at(-1);
			const page = await reviewQueue(pageSize, last ? `${last.created_at}:${last.id}` : undefined);
			submissions = submissions.concat(page);
			for (const submission of page) {
				reasons[submission.id] ??= reasonCodes[0];
			}
			hasMore = page.length === pageSize;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not load more submissions';
		} finally {
			more = false;
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

	const reasonOptions = reasonCodes.map((code) => ({ value: code, label: code }));
</script>

<svelte:head>
	<title>Review queue · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<PageHeader
		title="Review queue"
		subtitle="A decision applies to one release digest and never alters signed bytes. Accepting commits the entry to this home's feed."
	/>

	{#if error}
		<div role="alert" class="alert alert-error"><span>{error}</span></div>
	{/if}
	{#if notice}
		<div role="alert" class="alert alert-success"><span>{notice}</span></div>
	{/if}

	{#if loading}
		<div class="skeleton h-20 w-full"></div>
	{:else if submissions.length === 0}
		<EmptyState
			title="Nothing to review"
			message="Submissions waiting for a decision show up here."
		/>
	{:else}
		<div class="overflow-x-auto rounded-box border border-base-300">
			<table class="table table-sm">
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
							<td class="whitespace-nowrap">{formatTime(submission.created_at)}</td>
							<td>
								<span class="badge badge-ghost badge-sm">{submission.state}</span>
								{#if submission.assigned_to}
									<span class="block text-xs text-base-content/50">
										{submission.assigned_to === me ? 'yours' : 'another reviewer'}
									</span>
								{/if}
							</td>
							<td>
								<div class="w-44">
									<SelectField
										bare
										label="Reason code"
										bind:value={reasons[submission.id]}
										options={reasonOptions}
										placeholder="Reason code"
									/>
								</div>
							</td>
							<td>
								<div class="flex flex-wrap gap-1">
									{#if submission.state === 'submitted'}
										<button
											class="btn btn-sm btn-ghost"
											onclick={() => take(submission)}
											disabled={busy === submission.id}
										>
											<UserCheck size={15} />
											Take
										</button>
									{:else if submission.assigned_to !== me}
										<span class="text-xs text-base-content/50">held by another reviewer</span>
									{/if}
									{#if submission.state === 'submitted' || submission.assigned_to === me}
										<button
											class="btn btn-sm btn-success"
											onclick={() => act(submission, 'accept')}
											disabled={busy === submission.id}
										>
											<Check size={15} />
											Accept
										</button>
										<button
											class="btn btn-sm btn-outline"
											onclick={() => act(submission, 'reject')}
											disabled={busy === submission.id}
										>
											<X size={15} />
											Reject
										</button>
										<button
											class="btn btn-sm btn-outline"
											onclick={() => act(submission, 'quarantine')}
											disabled={busy === submission.id}
										>
											<ShieldAlert size={15} />
											Quarantine
										</button>
									{/if}
								</div>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
		{#if hasMore}
			<button class="btn btn-outline w-fit" type="button" onclick={loadMore} disabled={more}>
				Load more
			</button>
		{/if}
	{/if}
</div>
