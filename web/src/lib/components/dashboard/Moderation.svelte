<script lang="ts">
	import { onMount } from 'svelte';
	import { externalClaims, reviewExternalClaim, type ExternalClaim } from '$lib/api/external';
	import { safeExternalUrl } from '$lib/api/registry';
	import { authorizedFetch, failure } from '$lib/api/session';

	type Sanction = {
		id: string;
		subject_user_id: string;
		kind: string;
		reason_code: string;
		scope_kind: string;
		scope_id: string;
	};
	type Report = {
		id: string;
		claim_kind: string;
		claimant_ref: string;
		target_project_id?: string;
		target_handle?: string;
		status: string;
	};
	type LegalRequest = {
		id: string;
		kind: string;
		claimant_ref: string;
		target_kind: string;
		target_id: string;
		stated_basis: string;
		action_taken: string;
	};
	let sanctions = $state<Sanction[]>([]);
	let reports = $state<Report[]>([]);
	let legalRequests = $state<LegalRequest[]>([]);
	let claims = $state<ExternalClaim[]>([]);
	let legalTargetKind = $state('project');
	let legalTargetId = $state('');
	let error = $state<string | null>(null);
	let notice = $state<string | null>(null);

	onMount(load);

	async function load() {
		try {
			const [sanctionResponse, reportResponse, externalClaimRows] = await Promise.all([
				authorizedFetch('/v1/sanctions'),
				authorizedFetch('/v1/impersonation-reports'),
				externalClaims(),
			]);
			if (!sanctionResponse.ok) throw await failure(sanctionResponse);
			if (!reportResponse.ok) throw await failure(reportResponse);
			sanctions = await sanctionResponse.json();
			reports = await reportResponse.json();
			claims = externalClaimRows;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not load moderation records';
		}
	}

	async function submitSanction(event: SubmitEvent) {
		event.preventDefault();
		error = null;
		notice = null;
		const form = new FormData(event.currentTarget as HTMLFormElement);
		try {
			const response = await authorizedFetch('/v1/sanctions', {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify({
					subject_user_id: form.get('user'),
					kind: form.get('kind'),
					reason_code: form.get('reason'),
					scope_kind: form.get('scope'),
					scope_id: form.get('scope_id'),
					starts_at: Math.floor(Date.now() / 1000),
				}),
			});
			if (!response.ok) throw await failure(response);
			notice = 'Sanction recorded.';
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not record sanction';
		}
	}

	async function submitLegal(event: SubmitEvent) {
		event.preventDefault();
		error = null;
		notice = null;
		const form = new FormData(event.currentTarget as HTMLFormElement);
		try {
			const response = await authorizedFetch('/v1/legal-requests', {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify({
					kind: form.get('kind'),
					claimant_ref: form.get('claimant'),
					target_kind: legalTargetKind,
					target_id: legalTargetId,
					stated_basis: form.get('basis'),
					received_at: Math.floor(Date.now() / 1000),
				}),
			});
			if (!response.ok) throw await failure(response);
			const created = (await response.json()) as { request: LegalRequest };
			legalRequests = [created.request, ...legalRequests];
			notice = 'Legal request recorded.';
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not record legal request';
		}
	}

	async function loadLegalRequests() {
		error = null;
		notice = null;
		if (!legalTargetId.trim()) {
			error = 'enter a target id before loading legal requests';
			return;
		}
		try {
			const params = new URLSearchParams({
				target_kind: legalTargetKind,
				target_id: legalTargetId,
			});
			const response = await authorizedFetch(`/v1/legal-requests?${params}`);
			if (!response.ok) throw await failure(response);
			legalRequests = await response.json();
			notice = `Loaded ${legalRequests.length} legal request(s).`;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not load legal requests';
		}
	}

	async function submitReport(event: SubmitEvent) {
		event.preventDefault();
		error = null;
		notice = null;
		const form = new FormData(event.currentTarget as HTMLFormElement);
		try {
			const response = await authorizedFetch('/v1/impersonation-reports', {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify({
					claim_kind: form.get('claim_kind'),
					claimant_ref: form.get('claimant_ref'),
					target_project_id: form.get('target_project_id') || null,
					target_handle: form.get('target_handle') || null,
					evidence_ref: form.get('evidence_ref'),
				}),
			});
			if (!response.ok) throw await failure(response);
			reports = [(await response.json()) as Report, ...reports];
			notice = 'Impersonation report recorded.';
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not record impersonation report';
		}
	}

	async function pinProvider(event: SubmitEvent) {
		event.preventDefault();
		error = null;
		notice = null;
		const form = new FormData(event.currentTarget as HTMLFormElement);
		try {
			const response = await authorizedFetch(
				`/v1/providers/${encodeURIComponent(String(form.get('provider')))}/keys`,
				{
					method: 'POST',
					headers: { 'content-type': 'application/json' },
					body: JSON.stringify({ public_key: form.get('public_key') }),
				},
			);
			if (!response.ok) throw await failure(response);
			notice = 'Provider key pinned.';
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not pin provider key';
		}
	}

	async function decideClaim(claim: ExternalClaim, state: 'approved' | 'rejected') {
		error = null;
		notice = null;
		try {
			await reviewExternalClaim(claim.id, state);
			claims = claims.filter((item) => item.id !== claim.id);
			notice = `External project claim ${state}.`;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not review external project claim';
		}
	}
</script>

<div class="flex flex-col gap-6">
	{#if error}<div role="alert" class="alert alert-error"><span>{error}</span></div>{/if}
	{#if notice}<div role="alert" class="alert alert-success"><span>{notice}</span></div>{/if}
	<section class="flex flex-col gap-3">
		<h2 class="text-lg font-semibold">External project claims</h2>
		<p class="text-sm text-base-content/70">
			Claims are review records only. Approving one does not sign, import, or attach a native
			project.
		</p>
		{#if claims.length > 0}
			<ul class="flex flex-col gap-3">
				{#each claims as claim (claim.id)}
					<li class="rounded-box border border-base-300 p-3 text-sm">
						<div class="flex flex-wrap gap-x-3 gap-y-1">
							<span>{claim.provider}/{claim.external_project_id}</span>
							<span class="text-base-content/60">{claim.claimant_ref}</span>
							<span class="text-base-content/60"
								>expires {new Date(claim.expires_at * 1000).toLocaleString()}</span
							>
						</div>
						{#if safeExternalUrl(claim.challenge_ref)}
							<a
								class="link text-xs"
								href={safeExternalUrl(claim.challenge_ref) ?? undefined}
								target="_blank"
								rel="noopener noreferrer">Open challenge reference</a
							>
						{:else}
							<span class="text-xs text-base-content/60">Challenge reference is not a web URL.</span
							>
						{/if}
						<div class="mt-2 flex gap-2">
							<button
								class="btn btn-sm btn-primary"
								type="button"
								onclick={() => decideClaim(claim, 'approved')}>Approve</button
							>
							<button
								class="btn btn-sm btn-outline"
								type="button"
								onclick={() => decideClaim(claim, 'rejected')}>Reject</button
							>
						</div>
					</li>
				{/each}
			</ul>
		{:else}
			<p class="text-sm text-base-content/60">No pending external project claims.</p>
		{/if}
	</section>
	<section class="flex flex-col gap-3">
		<h2 class="text-lg font-semibold">Trusted evidence provider</h2>
		<p class="text-sm text-base-content/70">
			Pin the provider key before accepting its signed advisories or attestations.
		</p>
		<form class="grid gap-2 sm:grid-cols-2" onsubmit={pinProvider}>
			<input
				class="input"
				name="provider"
				required
				placeholder="scanner.example"
				aria-label="Provider id"
			/>
			<input
				class="input font-mono"
				name="public_key"
				required
				placeholder="64 hex characters"
				aria-label="Provider public key"
			/>
			<button class="btn w-fit" type="submit">Pin provider key</button>
		</form>
	</section>
	<section class="flex flex-col gap-3">
		<h2 class="text-lg font-semibold">Record an impersonation report</h2>
		<form class="grid gap-2 sm:grid-cols-2" onsubmit={submitReport}>
			<input
				class="input"
				name="claim_kind"
				required
				placeholder="impersonation"
				aria-label="Claim kind"
			/>
			<input
				class="input"
				name="claimant_ref"
				required
				placeholder="Claimant reference"
				aria-label="Report claimant"
			/>
			<input
				class="input"
				name="target_project_id"
				placeholder="gd:sha256:…"
				aria-label="Reported project id"
			/>
			<input
				class="input"
				name="target_handle"
				placeholder="project-handle"
				aria-label="Reported handle"
			/>
			<input
				class="input sm:col-span-2"
				name="evidence_ref"
				required
				placeholder="Evidence reference"
				aria-label="Report evidence"
			/>
			<button class="btn w-fit" type="submit">Record impersonation report</button>
		</form>
	</section>
	<section class="flex flex-col gap-3">
		<h2 class="text-lg font-semibold">Record a sanction</h2>
		<form class="grid gap-2 sm:grid-cols-2" onsubmit={submitSanction}>
			<input
				class="input"
				name="user"
				required
				placeholder="Subject account id"
				aria-label="Subject account id"
			/>
			<select class="select" name="kind" aria-label="Sanction kind"
				><option>warning</option><option>upload-restriction</option><option>suspension</option
				></select
			>
			<input
				class="input"
				name="reason"
				required
				placeholder="Reason code"
				aria-label="Reason code"
			/>
			<select class="select" name="scope" aria-label="Sanction scope"
				><option>account</option><option>project</option><option>release-digest</option><option
					>instance</option
				></select
			>
			<input class="input" name="scope_id" required placeholder="Scope id" aria-label="Scope id" />
			<button class="btn w-fit" type="submit">Record sanction</button>
		</form>
		{#if sanctions.length > 0}<ul class="flex flex-col gap-1 text-sm">
				{#each sanctions as item (item.id)}<li class="font-mono">
						{item.kind} · {item.subject_user_id} · {item.scope_kind}:{item.scope_id}
					</li>{/each}
			</ul>{:else}<p class="text-sm text-base-content/60">No sanctions recorded.</p>{/if}
	</section>
	<section class="flex flex-col gap-3">
		<h2 class="text-lg font-semibold">Record a legal request</h2>
		<form class="grid gap-2 sm:grid-cols-2" onsubmit={submitLegal}>
			<select class="select" name="kind" aria-label="Legal request kind"
				><option>copyright-notice</option><option>counter-notice</option><option>court-order</option
				><option>law-enforcement-request</option><option>platform-policy-action</option><option
					>other</option
				></select
			>
			<input
				class="input"
				name="claimant"
				required
				placeholder="Claimant reference"
				aria-label="Claimant reference"
			/>
			<select
				class="select"
				bind:value={legalTargetKind}
				name="target_kind"
				aria-label="Legal target kind"><option>project</option><option>release</option></select
			>
			<input
				class="input"
				name="target_id"
				required
				bind:value={legalTargetId}
				placeholder="Target id"
				aria-label="Legal target id"
			/>
			<input
				class="input sm:col-span-2"
				name="basis"
				required
				placeholder="Stated basis"
				aria-label="Stated basis"
			/>
			<div class="flex flex-wrap gap-2 sm:col-span-2">
				<button class="btn w-fit" type="submit">Record legal request</button>
				<button
					class="btn btn-outline w-fit"
					type="button"
					onclick={loadLegalRequests}
					disabled={!legalTargetId.trim()}
				>
					Load recorded requests
				</button>
			</div>
		</form>
		{#if legalRequests.length > 0}
			<ul class="flex flex-col gap-1 text-sm">
				{#each legalRequests as item (item.id)}<li>
						{item.kind} · {item.target_kind}:{item.target_id} · {item.action_taken}
					</li>{/each}
			</ul>
		{:else}<p class="text-sm text-base-content/60">No legal requests match this target.</p>{/if}
	</section>
	<section class="flex flex-col gap-3">
		<h2 class="text-lg font-semibold">Impersonation reports</h2>
		{#if reports.length > 0}<ul class="flex flex-col gap-1 text-sm">
				{#each reports as item (item.id)}<li class="font-mono">
						{item.status} · {item.claim_kind} · {item.target_project_id ??
							item.target_handle ??
							'unscoped'}
					</li>{/each}
			</ul>{:else}<p class="text-sm text-base-content/60">
				No impersonation reports recorded.
			</p>{/if}
	</section>
</div>
