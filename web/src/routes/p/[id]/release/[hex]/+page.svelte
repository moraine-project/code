<script lang="ts">
	import { Download } from '@lucide/svelte';
	import {
		blobUrl,
		digestHex,
		fileSha256,
		mirrorLocations,
		type ArtifactLocations,
	} from '$lib/api/registry';
	import Digest from '$lib/components/Digest.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let checking = $state(false);
	let checkResult = $state<{ file: string; match: boolean } | null>(null);
	let mirrors = $state<Record<string, ArtifactLocations | null>>({});

	async function loadMirrors(digest: string) {
		if (digest in mirrors) return;
		try {
			mirrors[digest] = await mirrorLocations(data.home, digest);
		} catch {
			mirrors[digest] = null;
		}
	}

	const primary = $derived(
		data.release?.artifacts.find((artifact) => artifact.is_primary) ??
			data.release?.artifacts[0] ??
			null,
	);

	function formatTime(seconds: number): string {
		return new Date(seconds * 1000).toLocaleString();
	}

	function formatBytes(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
		return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
	}

	function describeVersions(scheme: string, values: string[]): string {
		if (scheme === 'any' || values.length === 0) {
			return 'any version';
		}
		return values.join(', ');
	}

	async function checkFile(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		if (!file || !data.release) {
			return;
		}
		checking = true;
		checkResult = null;
		try {
			const actual = await fileSha256(file);
			const match = data.release.artifacts.some(
				(artifact) => digestHex(artifact.digest) === actual,
			);
			checkResult = { file: file.name, match };
		} finally {
			checking = false;
		}
	}
</script>

<svelte:head>
	<title>{data.release?.human_version ?? data.hex} · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<nav class="breadcrumbs text-sm">
		<ul>
			<li>
				<a href={`/p/${encodeURIComponent(data.projectId)}?home=${encodeURIComponent(data.home)}`}>
					{data.profile?.display_name ?? 'Project'}
				</a>
			</li>
			<li>{data.release?.human_version ?? data.hex}</li>
		</ul>
	</nav>

	{#if data.error}
		<div role="alert" class="alert alert-error"><span>{data.error}</span></div>
	{:else if data.release}
		{#each data.release.advisories ?? [] as advisory (advisory.advisory)}
			<div role="alert" class="alert alert-warning alert-soft">
				<span>
					<strong>{advisory.provider_id}</strong>
					reports {advisory.severity}
					{advisory.category}{advisory.block_promotion ? ' and blocks promotion' : ''}. Evidence,
					not a verdict.
				</span>
			</div>
		{/each}

		{#if data.release.withdrawal}
			<div role="alert" class="alert alert-error alert-soft">
				<span>
					<strong>Withdrawn by the publisher</strong>
					— {data.release.withdrawal.reason}{data.release.withdrawal.note
						? `: ${data.release.withdrawal.note}`
						: ''}
					({formatTime(data.release.withdrawal.declared_time)}).
				</span>
			</div>
		{/if}

		<header
			class="flex flex-col gap-5 rounded-box border border-base-300 bg-base-200 p-5 sm:flex-row sm:items-center sm:justify-between sm:p-6"
		>
			<div class="flex flex-col gap-2">
				<h1 class="text-2xl font-bold tracking-tight">{data.release.human_version}</h1>
				<div class="flex flex-wrap items-center gap-2">
					<span class="badge badge-ghost">{data.release.channel}</span>
					<span class="badge badge-ghost">{data.release.kind}</span>
					{#if data.release.license_expression}
						<span class="badge badge-outline">{data.release.license_expression}</span>
					{/if}
				</div>
				<p class="text-sm text-base-content/60">
					Published {formatTime(data.release.declared_time)}
				</p>
			</div>
			{#if primary && !data.release.withdrawal}
				<a class="btn btn-primary" href={blobUrl(data.home, primary.digest)}>
					<Download size={16} />
					Download
				</a>
			{/if}
		</header>

		{#if data.release.compatibility.length > 0}
			<section class="card card-border bg-base-200">
				<div class="card-body gap-3">
					<h2 class="card-title">Works with</h2>
					<ul class="flex flex-col gap-2">
						{#each data.release.compatibility as entry, index (index)}
							<li class="flex flex-wrap items-center gap-2 text-sm">
								{#each entry.values as value (value)}
									<span class="badge badge-outline">{value}</span>
								{/each}
								{#if entry.values.length === 0}
									<span class="text-base-content/60">Any game version</span>
								{/if}
								{#if entry.loader_id}
									<span class="text-base-content/50">·</span>
									<Digest value={entry.loader_id} label="the loader id" length={12} />
								{/if}
								<span class="badge badge-ghost badge-sm">
									{entry.side === 'both' ? 'client + server' : entry.side}
								</span>
							</li>
						{/each}
					</ul>
					<p class="text-xs text-base-content/50">
						Declared by the publisher for this release, not a test result.
					</p>
				</div>
			</section>
		{/if}

		<section class="card card-border bg-base-200">
			<div class="card-body gap-3">
				<h2 class="card-title">Files</h2>
				<div class="overflow-x-auto">
					<table class="table table-sm">
						<thead>
							<tr>
								<th scope="col">File</th>
								<th scope="col">Size</th>
								<th scope="col">Digest</th>
								<th scope="col"></th>
							</tr>
						</thead>
						<tbody>
							{#each data.release.artifacts as artifact (artifact.digest)}
								<tr>
									<td>
										<span class="font-medium">{artifact.filename}</span>
										{#if artifact.is_primary}
											<span class="badge badge-primary badge-sm ml-1">primary</span>
										{/if}
									</td>
									<td class="whitespace-nowrap">{formatBytes(artifact.size)}</td>
									<td
										><Digest
											value={artifact.digest}
											label={`the ${artifact.filename} digest`}
										/></td
									>
									<td>
										<div class="flex flex-wrap gap-2">
											<a class="btn btn-sm" href={blobUrl(data.home, artifact.digest)}>Download</a>
											<button
												class="btn btn-sm btn-ghost"
												type="button"
												onclick={() => loadMirrors(artifact.digest)}>Other locations</button
											>
										</div>
										{#if mirrors[artifact.digest]}
											<p class="mt-2 text-xs text-base-content/60">
												{mirrors[artifact.digest]?.locations.length ?? 0} location(s), {mirrors[
													artifact.digest
												]?.locations.filter(
													(location) => location.provenance === 'mirror-committed',
												).length ?? 0} verified mirror commitment(s).
											</p>
											<ul class="mt-1 flex flex-col gap-1 text-xs">
												{#each mirrors[artifact.digest]?.locations ?? [] as location (location.url)}
													<li>
														<a
															class="link"
															href={location.url}
															target="_blank"
															rel="noopener noreferrer">{location.kind}: {location.url}</a
														>
													</li>
												{/each}
											</ul>
										{:else if artifact.digest in mirrors}
											<p class="mt-2 text-xs text-base-content/60">
												No alternate locations are published.
											</p>
										{/if}
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			</div>
		</section>

		{#if data.changelog}
			<section class="card card-border bg-base-200">
				<div class="card-body gap-4">
					<h2 class="card-title">Release notes</h2>
					{#each data.changelog.locale_sections as locale (locale.locale)}
						<div class="flex flex-col gap-3">
							{#each locale.sections as section, index (index)}
								<div>
									<div class="flex flex-wrap items-center gap-2">
										<h3 class="font-semibold">{section.heading}</h3>
										{#if section.severity}
											<span class="badge badge-outline">{section.severity}</span>
										{/if}
									</div>
									<p class="whitespace-pre-wrap text-sm text-base-content/80">{section.body}</p>
								</div>
							{/each}
						</div>
					{/each}
				</div>
			</section>
		{/if}

		{#if (data.release.attestations ?? []).length > 0}
			<section class="card card-border bg-base-200">
				<div class="card-body gap-3">
					<h2 class="card-title">Evidence</h2>
					<ul class="flex flex-col gap-2 text-sm">
						{#each data.release.attestations ?? [] as entry (entry.attestation)}
							<li class="flex flex-wrap items-center gap-2">
								<span class="badge badge-outline">{entry.kind}</span>
								<span class="font-mono text-xs">{entry.signer_id}</span>
								<span class="text-base-content/50">
									{entry.media_type}{entry.has_inline_body ? ' · inline body' : ''} ·
									{formatTime(entry.issued_at)}
								</span>
							</li>
						{/each}
					</ul>
					<p class="text-xs text-base-content/50">
						Signed statements from the providers above. Not merged with the publisher's claims, and
						not a safety guarantee.
					</p>
				</div>
			</section>
		{/if}

		{#if data.release.dependencies.length > 0}
			<section class="card card-border bg-base-200">
				<div class="card-body gap-3">
					<h2 class="card-title">Dependencies</h2>
					<ul class="flex flex-col gap-1 text-sm">
						{#each data.release.dependencies as dependency, index (index)}
							<li class="flex flex-wrap items-center gap-2">
								<span class="badge badge-outline">{dependency.kind}</span>
								<span class="text-base-content/60">{dependency.target_kind}</span>
								<Digest value={dependency.target_id} label="the dependency id" length={12} />
							</li>
						{/each}
					</ul>
				</div>
			</section>
		{/if}

		{#if data.release.rights}
			<section class="card card-border bg-base-200">
				<div class="card-body gap-3">
					<h2 class="card-title">Rights</h2>
					<ul class="flex flex-wrap gap-2 text-sm">
						<li class="badge badge-outline">
							redistribution: {data.release.rights.redistribution}
						</li>
						<li class="badge badge-outline">modpacks: {data.release.rights.modpack_inclusion}</li>
						<li class="badge badge-outline">mirroring: {data.release.rights.mirroring}</li>
						{#if data.release.rights.attribution_required}
							<li class="badge badge-outline">attribution required</li>
						{/if}
					</ul>
				</div>
			</section>
		{/if}

		<details class="collapse-arrow collapse border border-base-300 bg-base-200">
			<summary class="collapse-title font-medium">Check a downloaded file</summary>
			<div class="collapse-content flex flex-col gap-3">
				<p class="text-sm text-base-content/70">
					Pick the file you saved. The browser hashes it locally and compares the bytes to this
					release's artifacts.
				</p>
				<div class="flex items-center gap-2">
					<input
						type="file"
						class="file-input w-full max-w-md"
						onchange={checkFile}
						disabled={checking}
						aria-label="File to check against this release"
					/>
					{#if checking}
						<span class="loading loading-spinner loading-sm" aria-hidden="true"></span>
					{/if}
				</div>
				{#if checkResult}
					{#if checkResult.match}
						<div role="alert" class="alert alert-success">
							<span>{checkResult.file} matches an artifact in this release.</span>
						</div>
					{:else}
						<div role="alert" class="alert alert-error">
							<span>{checkResult.file} does not match any artifact in this release.</span>
						</div>
					{/if}
					<p class="text-xs text-base-content/50">
						Byte check only. The verifier CLI checks the publisher's signature.
					</p>
				{/if}
			</div>
		</details>
	{/if}
</div>
