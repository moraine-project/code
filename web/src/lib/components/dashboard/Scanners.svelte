<script lang="ts">
	import { onMount } from 'svelte';
	import {
		createScannerSubscription,
		listScanJobs,
		listScannerPolicies,
		listScannerProviders,
		listScannerSubscriptions,
		registerScannerProvider,
		requestScan,
		rescan,
		saveScannerPolicy,
		type ScanJob,
		type ScannerProvider,
	} from '$lib/api/scanners';

	let providers = $state<ScannerProvider[]>([]);
	let jobs = $state<ScanJob[]>([]);
	let policies = $state<
		Array<{ id: string; provider_id: string; enabled: boolean; auto_scan: boolean }>
	>([]);
	let subscriptions = $state<Array<Record<string, unknown>>>([]);
	let providerId = $state('local-clamav');
	let kind = $state('clamav');
	let command = $state('clamscan');
	let args = $state('--no-summary');
	let publicKey = $state('');
	let digest = $state('');
	let endpoint = $state('');
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
			if (providers[0] && !providerId) providerId = providers[0].provider_id;
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
	function register(event: SubmitEvent) {
		event.preventDefault();
		return run(
			() =>
				registerScannerProvider({
					provider_id: providerId.trim(),
					kind: kind.trim(),
					command: command.trim(),
					args: args.trim() ? args.trim().split(/\s+/) : [],
					public_key: publicKey.trim(),
				}),
			'Scanner provider saved.',
		);
	}
	function scan(event: SubmitEvent) {
		event.preventDefault();
		return run(() => requestScan(digest.trim(), providerId.trim()), 'Scan queued.');
	}
</script>

<div class="flex flex-col gap-6">
	<p class="text-sm text-base-content/70">
		Scanners run as external commands. They never receive project signing keys; successful results
		become signed, attributed scanner attestations.
	</p>
	{#if error}<div role="alert" class="alert alert-error"><span>{error}</span></div>{/if}
	{#if notice}<div role="alert" class="alert alert-success"><span>{notice}</span></div>{/if}
	<section class="grid gap-4 lg:grid-cols-2">
		<form class="flex flex-col gap-3 rounded-box border border-base-300 p-4" onsubmit={register}>
			<h2 class="font-semibold">Scanner provider</h2>
			<label class="floating-label"
				><span>Provider ID</span><input class="input" bind:value={providerId} required /></label
			>
			<label class="floating-label"
				><span>Adapter kind</span><input class="input" bind:value={kind} required /></label
			>
			<label class="floating-label"
				><span>Executable</span><input class="input" bind:value={command} required /></label
			>
			<label class="floating-label"
				><span>Arguments</span><input class="input" bind:value={args} /></label
			>
			<label class="floating-label"
				><span>Provider public key (hex)</span><input
					class="input font-mono"
					bind:value={publicKey}
					required
				/></label
			>
			<button class="btn btn-primary w-fit" disabled={busy}>Save provider</button>
		</form>
		<form class="flex flex-col gap-3 rounded-box border border-base-300 p-4" onsubmit={scan}>
			<h2 class="font-semibold">Manual scan</h2>
			<label class="floating-label"
				><span>Artifact digest</span><input
					class="input font-mono"
					bind:value={digest}
					placeholder="sha256:…"
					required
				/></label
			>
			<label class="floating-label"
				><span>Provider</span><select class="select" bind:value={providerId} required
					>{#each providers as provider}<option value={provider.provider_id}
							>{provider.provider_id} · {provider.kind}</option
						>{/each}</select
				></label
			>
			<div class="flex flex-wrap gap-2">
				<button class="btn btn-primary" disabled={busy}>Scan this release</button><button
					class="btn btn-outline"
					type="button"
					disabled={busy || !jobs.length}
					onclick={() => run(() => rescan(jobs[0].id), 'Rescan queued.')}>Rescan latest</button
				>
			</div>
		</form>
	</section>
	<section class="rounded-box border border-base-300 p-4">
		<h2 class="font-semibold">Automatic scanning policy</h2>
		<form
			class="mt-3 flex flex-wrap items-end gap-3"
			onsubmit={(event) => {
				event.preventDefault();
				return run(
					() =>
						saveScannerPolicy({
							id: 'default',
							provider_id: providerId.trim(),
							enabled: true,
							auto_scan: true,
						}),
					'Automatic scanning policy saved.',
				);
			}}
		>
			<button class="btn btn-primary" disabled={busy}>Enable automatic scans</button>
		</form>
		{#if policies.length}<p class="mt-3 text-sm">{policies.length} policy(s) configured.</p>{/if}
	</section>
	<section class="rounded-box border border-base-300 p-4">
		<h2 class="font-semibold">External provider subscription</h2>
		<form
			class="mt-3 flex flex-wrap items-end gap-3"
			onsubmit={(event) => {
				event.preventDefault();
				return run(
					() => createScannerSubscription(providerId.trim(), endpoint.trim()),
					'Provider subscription saved.',
				);
			}}
		>
			<label class="floating-label"
				><span>Provider feed URL (HTTPS)</span><input
					class="input w-96"
					type="url"
					bind:value={endpoint}
					placeholder="https://scanner.example/feed"
					required
				/></label
			>
			<button class="btn btn-outline" disabled={busy}>Subscribe</button>
		</form>
		{#if subscriptions.length}<p class="mt-3 text-sm">
				{subscriptions.length} subscription(s) configured.
			</p>{/if}
	</section>
	<section class="rounded-box border border-base-300 p-4">
		<h2 class="font-semibold">Scan jobs</h2>
		{#if jobs.length}<div class="mt-3 overflow-x-auto">
				<table class="table table-sm">
					<thead><tr><th>Artifact</th><th>Provider</th><th>Status</th><th></th></tr></thead><tbody
						>{#each jobs as job}<tr
								><td class="font-mono text-xs">{job.artifact_digest}</td><td>{job.provider_id}</td
								><td>{job.status}</td><td
									><button
										class="btn btn-ghost btn-xs"
										onclick={() => run(() => rescan(job.id), 'Rescan queued.')}>Rescan</button
									></td
								></tr
							>{/each}</tbody
					>
				</table>
			</div>{:else}<p class="mt-3 text-sm text-base-content/70">No scan jobs yet.</p>{/if}
	</section>
</div>
