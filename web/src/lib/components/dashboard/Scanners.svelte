<script lang="ts">
	import { onMount } from 'svelte';
	import {
		createScannerSubscription,
		deleteScannerSubscription,
		listScanJobs,
		listScannerPolicies,
		listScannerProviders,
		listScannerSubscriptions,
		registerScannerProvider,
		requestScan,
		rescan,
		saveScannerPolicy,
		updateScannerSubscription,
		type ScanJob,
		type ScannerProvider,
		type ScannerSubscription,
	} from '$lib/api/scanners';

	type Policy = { id: string; provider_id: string; enabled: boolean; auto_scan: boolean };
	const adapterKinds = [
		{ value: 'clamav', label: 'ClamAV command adapter' },
		{ value: 'neko', label: 'Neko command adapter' },
		{ value: 'command', label: 'Generic command adapter' },
	];
	let providers = $state<ScannerProvider[]>([]);
	let jobs = $state<ScanJob[]>([]);
	let policies = $state<Policy[]>([]);
	let subscriptions = $state<ScannerSubscription[]>([]);
	let editingProviderId = $state('');
	let providerId = $state('local-clamav');
	let kind = $state('clamav');
	let command = $state('clamscan');
	let args = $state('--no-summary');
	let publicKey = $state('');
	let providerEnabled = $state(true);
	let digest = $state('');
	let scanProviderId = $state('');
	let policyId = $state('default');
	let policyProviderId = $state('');
	let policyEnabled = $state(true);
	let policyAutoScan = $state(true);
	let endpoint = $state('');
	let subscriptionProviderId = $state('');
	let intervalSeconds = $state(3600);
	let subscriptionEnabled = $state(true);
	let selectedJobId = $state('');
	let error = $state<string | null>(null);
	let notice = $state<string | null>(null);
	let busy = $state(false);

	onMount(load);
	async function load() {
		try {
			[providers, jobs, policies, subscriptions] = await Promise.all([
				listScannerProviders(),
				listScanJobs(),
				listScannerPolicies(),
				listScannerSubscriptions(),
			]);
			const firstProvider = providers[0]?.provider_id ?? '';
			scanProviderId ||= firstProvider;
			policyProviderId ||= firstProvider;
			subscriptionProviderId ||= firstProvider;
			if (!selectedJobId && jobs[0]) selectedJobId = jobs[0].id;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not load scanner configuration';
		}
	}
	async function run(action: () => Promise<unknown>, message: string) {
		busy = true;
		error = null;
		notice = null;
		try {
			await action();
			notice = message;
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'scanner operation failed';
		} finally {
			busy = false;
		}
	}
	function editProvider(provider: ScannerProvider) {
		editingProviderId = provider.provider_id;
		providerId = provider.provider_id;
		kind = provider.kind;
		command = provider.command;
		args = provider.args.join('\n');
		publicKey = provider.public_key;
		providerEnabled = provider.enabled;
	}
	function clearProviderForm() {
		editingProviderId = '';
		providerId = 'local-clamav';
		kind = 'clamav';
		command = 'clamscan';
		args = '--no-summary';
		publicKey = '';
		providerEnabled = true;
	}
	function register(event: SubmitEvent) {
		event.preventDefault();
		return run(
			() =>
				registerScannerProvider({
					provider_id: providerId.trim(),
					kind,
					command: command.trim(),
					args: args
						.split(/\r?\n/)
						.map((value) => value.trim())
						.filter(Boolean),
					public_key: publicKey.trim(),
					enabled: providerEnabled,
				}),
			'Scanner provider saved.',
		);
	}
	function scan(event: SubmitEvent) {
		event.preventDefault();
		return run(() => requestScan(digest.trim(), scanProviderId), 'Scan queued.');
	}
	function savePolicy(event: SubmitEvent) {
		event.preventDefault();
		return run(
			() =>
				saveScannerPolicy({
					id: policyId.trim(),
					provider_id: policyProviderId,
					enabled: policyEnabled,
					auto_scan: policyAutoScan,
				}),
			'Automatic scanning policy saved.',
		);
	}
	function editPolicy(policy: Policy) {
		policyId = policy.id;
		policyProviderId = policy.provider_id;
		policyEnabled = policy.enabled;
		policyAutoScan = policy.auto_scan;
	}
	function saveSubscription(subscription: ScannerSubscription) {
		return run(
			() =>
				updateScannerSubscription(subscription.id, {
					provider_id: subscription.provider_id,
					endpoint: subscription.endpoint,
					interval_seconds: subscription.interval_seconds,
					enabled: subscription.enabled,
				}),
			'Scanner subscription saved.',
		);
	}
	function createSubscription(event: SubmitEvent) {
		event.preventDefault();
		return run(
			() =>
				createScannerSubscription(
					subscriptionProviderId,
					endpoint.trim(),
					intervalSeconds,
					subscriptionEnabled,
				),
			'Scanner subscription created.',
		);
	}
	function selectedJob() {
		return jobs.find((job) => job.id === selectedJobId);
	}
	function rawEvidence(job: ScanJob) {
		return job.result?.raw ? JSON.stringify(job.result.raw, null, 2) : '';
	}
</script>

<div class="flex flex-col gap-6">
	<p class="max-w-3xl text-sm text-base-content/70">
		Providers are selected adapter types. The adapters currently available here execute scanner
		commands; the same boundary can later host API-backed adapters. Successful results become
		signed, attributed scanner attestations and never receive project signing keys.
	</p>
	{#if error}<div role="alert" class="alert alert-error"><span>{error}</span></div>{/if}
	{#if notice}<div role="alert" class="alert alert-success"><span>{notice}</span></div>{/if}

	<section class="grid gap-4 lg:grid-cols-2">
		<form class="flex flex-col gap-3 rounded-box border border-base-300 p-4" onsubmit={register}>
			<div class="flex items-start justify-between gap-3">
				<div>
					<h2 class="font-semibold">Scanner provider</h2>
					<p class="text-sm text-base-content/60">Select an adapter and configure its trust key.</p>
				</div>
				{#if editingProviderId}<button
						class="btn btn-ghost btn-xs"
						type="button"
						onclick={clearProviderForm}>New provider</button
					>{/if}
			</div>
			<label class="floating-label"
				><span>Provider ID</span><input class="input" bind:value={providerId} required /></label
			>
			<label class="floating-label"
				><span>Adapter type</span><select class="select" bind:value={kind} required
					>{#each adapterKinds as adapter}<option value={adapter.value}>{adapter.label}</option
						>{/each}</select
				></label
			>
			<div class="rounded-box bg-base-200 p-3 text-sm text-base-content/70">
				{#if kind === 'clamav'}ClamAV uses exit code 0 for clean, 1 for a finding, and other codes
					for errors.{:else if kind === 'neko'}Neko output is checked for known finding markers and
					non-zero exit status.{:else}The generic adapter maps exit code 0 to clean and 1 to
					finding.{/if}
			</div>
			<label class="floating-label"
				><span>Executable</span><input class="input" bind:value={command} required /></label
			>
			<label class="floating-label"
				><span>Arguments, one per line</span><textarea
					class="textarea min-h-20 font-mono"
					bind:value={args}></textarea></label
			>
			<label class="floating-label"
				><span>Provider public key (hex)</span><input
					class="input font-mono"
					bind:value={publicKey}
					required
				/></label
			>
			<label class="label cursor-pointer justify-start gap-3"
				><input class="checkbox" type="checkbox" bind:checked={providerEnabled} /><span
					>Provider enabled</span
				></label
			>
			<button class="btn btn-primary w-fit" disabled={busy}>Save provider</button>
		</form>
		<section class="rounded-box border border-base-300 p-4">
			<h2 class="font-semibold">Registered providers</h2>
			{#if providers.length}<div class="mt-3 flex flex-col gap-2">
					{#each providers as provider}<button
							class="flex items-center justify-between rounded-box border border-base-300 p-3 text-left hover:bg-base-200"
							class:border-primary={provider.provider_id === editingProviderId}
							onclick={() => editProvider(provider)}
							><span
								><strong>{provider.provider_id}</strong><span
									class="ml-2 text-sm text-base-content/60">{provider.kind}</span
								></span
							><span class="badge" class:badge-success={provider.enabled}
								>{provider.enabled ? 'enabled' : 'disabled'}</span
							></button
						>{/each}
				</div>{:else}<p class="mt-3 text-sm text-base-content/70">No providers registered.</p>{/if}
		</section>
	</section>

	<section class="rounded-box border border-base-300 p-4">
		<h2 class="font-semibold">Manual scan</h2>
		<form class="mt-3 flex flex-wrap items-end gap-3" onsubmit={scan}>
			<label class="floating-label"
				><span>Artifact digest</span><input
					class="input font-mono"
					bind:value={digest}
					placeholder="sha256:…"
					required
				/></label
			><label class="floating-label"
				><span>Provider</span><select class="select" bind:value={scanProviderId} required
					>{#each providers as provider}<option
							value={provider.provider_id}
							disabled={!provider.enabled}
							>{provider.provider_id} · {provider.kind}{provider.enabled
								? ''
								: ' · disabled'}</option
						>{/each}</select
				></label
			><button
				class="btn btn-primary"
				disabled={busy || !providers.some((provider) => provider.enabled)}>Queue scan</button
			>
		</form>
	</section>

	<section class="rounded-box border border-base-300 p-4">
		<h2 class="font-semibold">Automatic scanning policies</h2>
		<p class="text-sm text-base-content/60">
			Each policy controls whether new artifacts are queued for a provider.
		</p>
		<form class="mt-3 flex flex-wrap items-end gap-3" onsubmit={savePolicy}>
			<label class="floating-label"
				><span>Policy ID</span><input class="input" bind:value={policyId} required /></label
			><label class="floating-label"
				><span>Provider</span><select class="select" bind:value={policyProviderId} required
					>{#each providers as provider}<option value={provider.provider_id}
							>{provider.provider_id}</option
						>{/each}</select
				></label
			><label class="label cursor-pointer gap-2"
				><input class="checkbox" type="checkbox" bind:checked={policyEnabled} />Enabled</label
			><label class="label cursor-pointer gap-2"
				><input class="checkbox" type="checkbox" bind:checked={policyAutoScan} />Auto scan</label
			><button class="btn btn-primary" disabled={busy}>Save policy</button>
		</form>
		{#if policies.length}<div class="mt-4 overflow-x-auto">
				<table class="table table-sm">
					<thead
						><tr><th>Policy</th><th>Provider</th><th>Enabled</th><th>Auto scan</th><th></th></tr
						></thead
					><tbody
						>{#each policies as policy}<tr
								><td class="font-mono">{policy.id}</td><td>{policy.provider_id}</td><td
									>{policy.enabled ? 'yes' : 'no'}</td
								><td>{policy.auto_scan ? 'yes' : 'no'}</td><td
									><button class="btn btn-ghost btn-xs" onclick={() => editPolicy(policy)}
										>Edit</button
									></td
								></tr
							>{/each}</tbody
					>
				</table>
			</div>{:else}<p class="mt-3 text-sm text-base-content/70">No policies configured.</p>{/if}
	</section>

	<section class="rounded-box border border-base-300 p-4">
		<h2 class="font-semibold">External provider subscriptions</h2>
		<p class="text-sm text-base-content/60">
			Subscriptions import signed scanner attestations from HTTPS feeds.
		</p>
		<form class="mt-3 flex flex-wrap items-end gap-3" onsubmit={createSubscription}>
			<label class="floating-label"
				><span>Provider</span><select class="select" bind:value={subscriptionProviderId} required
					>{#each providers as provider}<option value={provider.provider_id}
							>{provider.provider_id}</option
						>{/each}</select
				></label
			><label class="floating-label"
				><span>Feed URL (HTTPS)</span><input
					class="input w-80"
					type="url"
					bind:value={endpoint}
					placeholder="https://scanner.example/feed"
					required
				/></label
			><label class="floating-label"
				><span>Interval (seconds)</span><input
					class="input w-36"
					type="number"
					min="60"
					max="86400"
					bind:value={intervalSeconds}
					required
				/></label
			><label class="label cursor-pointer gap-2"
				><input class="checkbox" type="checkbox" bind:checked={subscriptionEnabled} />Enabled</label
			><button class="btn btn-outline" disabled={busy}>Add subscription</button>
		</form>
		{#if subscriptions.length}<div class="mt-4 overflow-x-auto">
				<table class="table table-sm">
					<thead
						><tr
							><th>Provider</th><th>Feed</th><th>Interval</th><th>Enabled</th><th>Last poll</th><th
							></th></tr
						></thead
					><tbody
						>{#each subscriptions as subscription}<tr
								><td
									><select class="select select-sm" bind:value={subscription.provider_id}
										>{#each providers as provider}<option value={provider.provider_id}
												>{provider.provider_id}</option
											>{/each}</select
									></td
								><td
									><input
										class="input input-sm w-72"
										type="url"
										bind:value={subscription.endpoint}
									/></td
								><td
									><input
										class="input input-sm w-28"
										type="number"
										min="60"
										max="86400"
										bind:value={subscription.interval_seconds}
									/></td
								><td
									><input
										class="checkbox"
										type="checkbox"
										bind:checked={subscription.enabled}
										aria-label={`Enable ${subscription.id}`}
									/></td
								><td
									>{subscription.last_polled_at
										? new Date(subscription.last_polled_at * 1000).toLocaleString()
										: 'Never'}</td
								><td class="flex gap-1"
									><button
										class="btn btn-ghost btn-xs"
										disabled={busy}
										onclick={() => saveSubscription(subscription)}>Save</button
									><button
										class="btn btn-ghost btn-xs text-error"
										disabled={busy}
										onclick={() =>
											run(
												() => deleteScannerSubscription(subscription.id),
												'Subscription deleted.',
											)}>Delete</button
									></td
								></tr
							>{/each}</tbody
					>
				</table>
			</div>{:else}<p class="mt-3 text-sm text-base-content/70">
				No subscriptions configured.
			</p>{/if}
	</section>

	<section class="rounded-box border border-base-300 p-4">
		<div class="flex items-center justify-between gap-3">
			<h2 class="font-semibold">Scan jobs</h2>
			<span class="text-sm text-base-content/60">{jobs.length} job(s)</span>
		</div>
		{#if jobs.length}<div class="mt-3 overflow-x-auto">
				<table class="table table-sm">
					<thead
						><tr
							><th></th><th>Artifact</th><th>Provider</th><th>Status</th><th>Verdict</th><th
								>Findings</th
							><th></th></tr
						></thead
					><tbody
						>{#each jobs as job}<tr class:bg-base-200={job.id === selectedJobId}
								><td
									><input
										class="radio"
										type="radio"
										name="selected-scan"
										value={job.id}
										bind:group={selectedJobId}
										aria-label={`Select ${job.id}`}
									/></td
								><td class="font-mono text-xs">{job.artifact_digest}</td><td>{job.provider_id}</td
								><td>{job.status}</td><td>{job.result?.verdict ?? '—'}</td><td
									>{job.result?.findings.length ?? '—'}</td
								><td
									><button
										class="btn btn-ghost btn-xs"
										disabled={busy}
										onclick={() => run(() => rescan(job.id), 'Rescan queued.')}>Rescan</button
									></td
								></tr
							>{/each}</tbody
					>
				</table>
			</div>
			{#if selectedJob()}<div class="mt-4 rounded-box bg-base-200 p-4">
					<div class="flex flex-wrap items-center justify-between gap-3">
						<h3 class="font-semibold">Selected result</h3>
						<button
							class="btn btn-outline btn-sm"
							disabled={busy}
							onclick={() => run(() => rescan(selectedJobId), 'Rescan queued.')}
							>Rescan selected job</button
						>
					</div>
					<dl class="mt-3 grid gap-2 text-sm sm:grid-cols-3">
						<div>
							<dt class="text-base-content/60">Status</dt>
							<dd>{selectedJob()?.status}</dd>
						</div>
						<div>
							<dt class="text-base-content/60">Verdict</dt>
							<dd>{selectedJob()?.result?.verdict ?? 'No result yet'}</dd>
						</div>
						<div>
							<dt class="text-base-content/60">Exit code</dt>
							<dd>{selectedJob()?.result?.exit_code ?? '—'}</dd>
						</div>
					</dl>
					{#if selectedJob()?.error}<p class="mt-3 text-error">
							{selectedJob()?.error}
						</p>{/if}{#if selectedJob()?.result?.findings.length}<div class="mt-3">
							<h4 class="font-medium">Findings</h4>
							<ul class="list-disc pl-5 text-sm">
								{#each selectedJob()?.result?.findings ?? [] as finding}<li>
										{String(finding.message ?? JSON.stringify(finding))}
									</li>{/each}
							</ul>
						</div>{/if}{#if rawEvidence(selectedJob() ?? jobs[0])}<details class="mt-3">
							<summary class="cursor-pointer text-sm font-medium">Raw scanner evidence</summary>
							<pre class="mt-2 max-h-80 overflow-auto whitespace-pre-wrap text-xs">{rawEvidence(
									selectedJob() ?? jobs[0],
								)}</pre>
						</details>{/if}
				</div>{/if}{:else}<p class="mt-3 text-sm text-base-content/70">No scan jobs yet.</p>{/if}
	</section>
</div>
