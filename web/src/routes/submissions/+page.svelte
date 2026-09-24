<script lang="ts">
	import { onMount } from 'svelte';
	import { CheckCircle2, Clock, Inbox, ShieldAlert, XCircle } from '@lucide/svelte';
	import Digest from '$lib/components/Digest.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import PageHeader from '$lib/components/PageHeader.svelte';
	import { mySubmissions, type SubmissionDetail } from '$lib/api/review';
	import { safeExternalUrl } from '$lib/api/external-url';
	import { pageTitle } from '$lib/title.svelte';

	const pageSize = 50;

	let items = $state<SubmissionDetail[]>([]);
	let error = $state<string | null>(null);
	let loading = $state(true);
	let more = $state(false);
	let hasMore = $state(false);

	onMount(load);

	async function load() {
		loading = true;
		try {
			items = await mySubmissions(pageSize);
			hasMore = items.length === pageSize;
			error = null;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not load your submissions';
		} finally {
			loading = false;
		}
	}

	async function loadMore() {
		more = true;
		try {
			const last = items.at(-1)?.submission;
			const page = await mySubmissions(
				pageSize,
				last ? `${last.created_at}:${last.id}` : undefined,
			);
			items = items.concat(page);
			hasMore = page.length === pageSize;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not load more submissions';
		} finally {
			more = false;
		}
	}

	function formatTime(seconds: number): string {
		return new Date(seconds * 1000).toLocaleString();
	}

	function stateBadge(state: string): string {
		if (state === 'accepted') return 'badge badge-success badge-sm';
		if (state === 'rejected') return 'badge badge-error badge-sm';
		if (state === 'quarantined') return 'badge badge-warning badge-sm';
		return 'badge badge-ghost badge-sm';
	}

	function stateIcon(state: string) {
		if (state === 'accepted') return CheckCircle2;
		if (state === 'rejected') return XCircle;
		if (state === 'quarantined') return ShieldAlert;
		return Clock;
	}
</script>

<svelte:head>
	<title>{pageTitle('Your submissions')}</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<PageHeader
		title="Submissions"
		subtitle="Acceptance is this instance's listing decision, not a safety guarantee. A rejection keeps the signed record and states a reason."
	/>

	{#if error}
		<div role="alert" class="alert alert-error"><span>{error}</span></div>
	{/if}

	{#if loading}
		<div class="skeleton h-20 w-full"></div>
	{:else if items.length === 0}
		<EmptyState
			title="No submissions yet"
			message="Publish a release and it appears here while this instance reviews it."
		>
			<a class="btn btn-primary" href="/publish">Publish a mod</a>
		</EmptyState>
	{:else}
		<ul class="flex flex-col gap-3">
			{#each items as item (item.submission.id)}
				{@const Icon = stateIcon(item.submission.state)}
				<li class="card card-border bg-base-200">
					<div class="card-body gap-3 p-4">
						<div class="flex flex-wrap items-center justify-between gap-2">
							<div class="flex flex-wrap items-center gap-2">
								<span class={stateBadge(item.submission.state)}>
									<Icon size={13} />
									{item.submission.state}
								</span>
								<Digest value={item.submission.object} label="the object id" />
							</div>
							<span class="text-xs text-base-content/60">
								{formatTime(item.submission.created_at)}
							</span>
						</div>
						{#each item.decisions as decision (decision.decided_at)}
							{@const appeal = decision.appeal_route
								? safeExternalUrl(decision.appeal_route)
								: null}
							<p class="text-sm text-base-content/80">
								<strong>Decision:</strong>
								{decision.decision}
								{#if decision.reason_code}
									— {decision.reason_code}
								{/if}
								{#if decision.appeal_route}
									· appeal:
									{#if appeal}
										<a class="link" href={appeal} rel="noopener noreferrer" target="_blank"
											>{decision.appeal_route}</a
										>
									{:else}
										<span>{decision.appeal_route}</span>
									{/if}
								{/if}
							</p>
						{/each}
					</div>
				</li>
			{/each}
		</ul>
		{#if hasMore}
			<button class="btn btn-outline w-fit" type="button" onclick={loadMore} disabled={more}>
				<Inbox size={16} />
				Load older submissions
			</button>
		{/if}
	{/if}
</div>
