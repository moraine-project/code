<script lang="ts">
	import { blobUrl, digestHex, fileSha256 } from '$lib/api/registry';
	import Digest from '$lib/components/Digest.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let checking = $state(false);
	let checkResult = $state<{ file: string; match: boolean } | null>(null);

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
		if (scheme === 'semver') {
			return values.join(' and ');
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
	<a
		class="link link-hover w-fit"
		href={`/p/${encodeURIComponent(data.projectId)}?home=${encodeURIComponent(data.home)}`}
	>
		← Back to project
	</a>

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
					({formatTime(data.release.withdrawal.declared_time)}). The release record and its digest
					are unchanged.
				</span>
			</div>
		{/if}

		<section class="card card-border bg-base-200">
			<div class="card-body gap-4">
				<div class="flex flex-wrap items-start justify-between gap-4">
					<div class="flex flex-col gap-2">
						<p class="text-sm text-base-content/70">
							<a
								class="link link-hover"
								href={`/p/${encodeURIComponent(data.projectId)}?home=${encodeURIComponent(data.home)}`}
							>
								{data.profile?.display_name ?? 'This project'}
							</a>
						</p>
						<h1 class="text-2xl font-semibold tracking-tight">{data.release.human_version}</h1>
						<div class="flex flex-wrap gap-2">
							<span class="badge badge-outline">{data.release.channel}</span>
							<span class="badge badge-outline">{data.release.kind}</span>
							{#if data.release.license_expression}
								<span class="badge badge-ghost">{data.release.license_expression}</span>
							{/if}
						</div>
						<p class="text-sm text-base-content/60">
							Declared {formatTime(data.release.declared_time)}
						</p>
					</div>
					{#if primary && !data.release.withdrawal}
						<a class="btn btn-primary" href={blobUrl(data.home, primary.digest)}>
							Download {primary.filename}
						</a>
					{/if}
				</div>
			</div>
		</section>

		<section class="card card-border">
			<div class="card-body">
				<h2 class="card-title">Files in this release</h2>
				<div class="overflow-x-auto">
					<table class="table table-sm">
						<thead>
							<tr>
								<th scope="col">File</th>
								<th scope="col">Digest</th>
								<th scope="col">Size</th>
								<th scope="col"></th>
							</tr>
						</thead>
						<tbody>
							{#each data.release.artifacts as artifact (artifact.digest)}
								<tr>
									<td>
										{artifact.filename}
										{#if artifact.is_primary}
											<span class="badge badge-ghost badge-sm">primary</span>
										{/if}
									</td>
									<td
										><Digest
											value={artifact.digest}
											label={`the ${artifact.filename} digest`}
										/></td
									>
									<td>{formatBytes(artifact.size)}</td>
									<td>
										<a class="btn btn-sm" href={blobUrl(data.home, artifact.digest)}>Download</a>
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
				<p class="text-base-content/60 text-sm">
					Downloading gets you the bytes. It does not prove the file is safe; use the verifier tool
					to check the publisher's signature and the file fingerprint.
				</p>
			</div>
		</section>

		<section class="card card-border">
			<div class="card-body">
				<h2 class="card-title">Check a downloaded file</h2>
				<p class="text-base-content/80 text-sm">
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
						<span class="sr-only" role="status">Hashing the file</span>
					{/if}
				</div>
				{#if checkResult}
					{#if checkResult.match}
						<div role="alert" class="alert alert-success">
							<span>{checkResult.file} matches an artifact in this release.</span>
						</div>
					{:else}
						<div role="alert" class="alert alert-error">
							<span>{checkResult.file} does not match any artifact digest in this release.</span>
						</div>
					{/if}
					<p class="text-base-content/60 text-sm">
						This checks the bytes only. It does not check the publisher's signature; the verifier
						CLI does that.
					</p>
				{/if}
			</div>
		</section>

		{#if data.release.compatibility.length > 0}
			<section class="card card-border bg-base-200">
				<div class="card-body gap-2">
					<h2 class="card-title">Works with</h2>
					<p class="text-sm text-base-content/70">
						Declared by the publisher for this release. A declaration is a claim by the author, not
						a test result.
					</p>
					<ul class="flex flex-col gap-1 text-sm">
						{#each data.release.compatibility as entry, index (index)}
							<li class="flex flex-wrap items-center gap-2">
								<span>Game {describeVersions(entry.scheme, entry.values)}</span>
								{#if entry.loader_id}
									<span class="text-base-content/60">with loader</span>
									<Digest value={entry.loader_id} label="the loader id" length={12} />
								{/if}
								<span class="badge badge-ghost badge-sm">
									{entry.side === 'both' ? 'client and server' : entry.side}
								</span>
							</li>
						{/each}
					</ul>
				</div>
			</section>
		{/if}

		{#if (data.release.attestations ?? []).length > 0}
			<section class="card card-border">
				<div class="card-body">
					<h2 class="card-title">Evidence</h2>
					<p class="text-base-content/60 text-sm">
						Signed statements about this release's artifacts from the providers named below.
						Compatibility above is declared by the publisher; evidence is not merged into it, and
						neither is a safety guarantee.
					</p>
					<ul class="flex flex-col gap-2 text-sm">
						{#each data.release.attestations ?? [] as entry (entry.attestation)}
							<li>
								<span class="badge badge-outline">{entry.kind}</span>
								<span class="font-mono">{entry.signer_id}</span>
								<span class="text-base-content/60">
									{entry.media_type}{entry.has_inline_body ? ' · inline body' : ''}
									· {formatTime(entry.issued_at)}
								</span>
							</li>
						{/each}
					</ul>
				</div>
			</section>
		{/if}

		{#if data.release.dependencies.length > 0}
			<section class="card card-border">
				<div class="card-body">
					<h2 class="card-title">Dependencies</h2>
					<ul class="flex flex-col gap-1 text-sm">
						{#each data.release.dependencies as dependency, index (index)}
							<li>
								<span class="badge badge-outline">{dependency.kind}</span>
								<span class="text-base-content/60">{dependency.target_kind}</span>
								<Digest value={dependency.target_id} label="the dependency id" length={12} />
							</li>
						{/each}
					</ul>
				</div>
			</section>
		{/if}

		{#if data.changelog}
			<section class="card card-border">
				<div class="card-body">
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
									<p class="whitespace-pre-wrap text-sm">{section.body}</p>
								</div>
							{/each}
						</div>
					{/each}
					<p class="text-base-content/60 text-sm">
						These notes are signed by the publisher and referenced from the release. A directory can
						show them, but it cannot change them.
					</p>
				</div>
			</section>
		{/if}

		{#if data.release.rights}
			<section class="card card-border">
				<div class="card-body">
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
	{/if}
</div>
