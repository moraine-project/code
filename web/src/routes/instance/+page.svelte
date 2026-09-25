<script lang="ts">
	import PageHeader from '$lib/components/PageHeader.svelte';
	import { instance } from '$lib/instance.svelte';
	import { pageTitle } from '$lib/title.svelte';

	const capabilities = $derived(instance.capabilities);

	function formatBytes(bytes: number | null): string {
		if (bytes === null) return 'Not published';
		if (bytes < 1024) return `${bytes} B`;
		const units = ['KiB', 'MiB', 'GiB', 'TiB'];
		let value = bytes;
		let unit = 0;
		while (value >= 1024 && unit < units.length - 1) {
			value /= 1024;
			unit += 1;
		}
		return `${value.toFixed(1)} ${units[unit] ?? 'KiB'}`;
	}

	function formatSeconds(seconds: number | null): string {
		if (seconds === null) return 'Not published';
		if (seconds === 0) return '0 seconds';
		if (seconds % 3600 === 0) return `${seconds / 3600} hour${seconds === 3600 ? '' : 's'}`;
		if (seconds % 60 === 0) return `${seconds / 60} minute${seconds === 60 ? '' : 's'}`;
		return `${seconds} seconds`;
	}

	function formatRate(value: number | null): string {
		if (value === null) return 'Not published';
		return value === 0 ? 'Unlimited (0 per minute)' : `${value} per minute`;
	}

	function formatList(values: string[]): string {
		return values.length > 0 ? values.join(', ') : 'None published';
	}
</script>

<svelte:head><title>{pageTitle('Instance information')}</title></svelte:head>

<div class="flex flex-col gap-6">
	<PageHeader
		title="Instance information"
		subtitle="The capabilities and limits this home advertises publicly."
	/>

	<section class="card card-border bg-base-200">
		<div class="card-body gap-5">
			<div>
				<h2 class="card-title">Capabilities</h2>
				<p class="mt-1 text-sm text-base-content/70">
					These values come from this home's public instance document.
				</p>
			</div>
			<dl class="grid gap-4 sm:grid-cols-2">
				<div>
					<dt class="text-xs text-base-content/60">Protocol versions</dt>
					<dd>{formatList(capabilities.protocolVersions.map(String))}</dd>
				</div>
				<div>
					<dt class="text-xs text-base-content/60">Server roles</dt>
					<dd>{formatList(capabilities.serverRole)}</dd>
				</div>
				<div>
					<dt class="text-xs text-base-content/60">Artifact sources</dt>
					<dd>{formatList(capabilities.artifactSources)}</dd>
				</div>
				<div>
					<dt class="text-xs text-base-content/60">Upload modes</dt>
					<dd>{formatList(capabilities.uploadModes)}</dd>
				</div>
				<div class="sm:col-span-2">
					<dt class="text-xs text-base-content/60">Webhook public key</dt>
					<dd class="break-all font-mono text-xs">
						{capabilities.webhookPublicKey ?? 'Not published'}
					</dd>
				</div>
			</dl>
		</div>
	</section>

	<section class="card card-border bg-base-200">
		<div class="card-body gap-5">
			<div>
				<h2 class="card-title">Limits</h2>
				<p class="mt-1 text-sm text-base-content/70">
					These are advertised request and storage bounds, not guarantees about a particular
					project.
				</p>
			</div>
			<dl class="grid gap-4 sm:grid-cols-2">
				<div>
					<dt class="text-xs text-base-content/60">Maximum artifact size</dt>
					<dd>{formatBytes(capabilities.maxArtifactBytes)}</dd>
				</div>
				<div>
					<dt class="text-xs text-base-content/60">Account upload quota</dt>
					<dd>{formatBytes(capabilities.maxUploadBytesPerAccount)}</dd>
				</div>
				<div>
					<dt class="text-xs text-base-content/60">Feed page entries</dt>
					<dd>
						{capabilities.maxFeedPageEntries === 0
							? 'Unlimited'
							: (capabilities.maxFeedPageEntries ?? 'Not published')}
					</dd>
				</div>
				<div>
					<dt class="text-xs text-base-content/60">Feed scan pages</dt>
					<dd>{capabilities.maxFeedScanPages ?? 'Not published'}</dd>
				</div>
				<div>
					<dt class="text-xs text-base-content/60">Maximum response size</dt>
					<dd>{formatBytes(capabilities.maxResponseBytes)}</dd>
				</div>
				<div>
					<dt class="text-xs text-base-content/60">Maximum sync pages</dt>
					<dd>{capabilities.maxSyncPages ?? 'Not published'}</dd>
				</div>
				<div>
					<dt class="text-xs text-base-content/60">Request rate</dt>
					<dd>{formatRate(capabilities.requestsPerMinute)}</dd>
				</div>
				<div>
					<dt class="text-xs text-base-content/60">Concurrent syncs</dt>
					<dd>{capabilities.maxConcurrentSyncs ?? 'Not published'}</dd>
				</div>
				<div class="sm:col-span-2">
					<dt class="text-xs text-base-content/60">Maintenance interval</dt>
					<dd>{formatSeconds(capabilities.maintenanceIntervalSeconds)}</dd>
				</div>
			</dl>
		</div>
	</section>

	<section class="card card-border bg-base-200">
		<div class="card-body gap-5">
			<div>
				<h2 class="card-title">Publication policy</h2>
				<p class="mt-1 text-sm text-base-content/70">
					These are the instance-wide publication and account settings advertised by this home.
					Individual project listing decisions are reported on project and search responses.
				</p>
			</div>
			<dl class="grid gap-4 sm:grid-cols-3">
				<div>
					<dt class="text-xs text-base-content/60">Publishing</dt>
					<dd>{capabilities.publishing}</dd>
				</div>
				<div>
					<dt class="text-xs text-base-content/60">Registration</dt>
					<dd>{capabilities.registration}</dd>
				</div>
				<div>
					<dt class="text-xs text-base-content/60">Email verification</dt>
					<dd>{capabilities.emailVerification ? 'Available' : 'Not available'}</dd>
				</div>
			</dl>
		</div>
	</section>

	<section class="card card-border bg-base-200">
		<div class="card-body gap-2">
			<h2 class="card-title">Contact</h2>
			<p>No contact address is published in this instance document.</p>
		</div>
	</section>
</div>
