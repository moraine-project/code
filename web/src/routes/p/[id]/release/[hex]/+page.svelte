<script lang="ts">
	import { blobUrl, digestHex, fileSha256 } from '$lib/api/registry';
	import Digest from '$lib/components/Digest.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let checking = $state(false);
	let checkResult = $state<{ file: string; match: boolean } | null>(null);

	function formatTime(seconds: number): string {
		return new Date(seconds * 1000).toLocaleString();
	}

	function formatBytes(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
		return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
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
			<div role="alert" class="alert alert-warning">
				<span>
					<strong>{advisory.provider_id}</strong>
					reports {advisory.severity}
					{advisory.category}{advisory.block_promotion ? ' and blocks promotion' : ''}. Evidence,
					not a verdict.
				</span>
			</div>
		{/each}

		{#if data.release.withdrawal}
			<div role="alert" class="alert alert-error">
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

		<section class="card card-border">
			<div class="card-body">
				<h1 class="card-title">{data.release.human_version}</h1>
				<div class="flex flex-wrap gap-2">
					<span class="badge badge-outline">{data.release.channel}</span>
					<span class="badge badge-outline">{data.release.kind}</span>
					{#if data.release.license_expression}
						<span class="badge">{data.release.license_expression}</span>
					{/if}
				</div>
				<p class="text-base-content/60 text-sm">
					Declared {formatTime(data.release.declared_time)}
				</p>
			</div>
		</section>

		<section class="card card-border">
			<div class="card-body">
				<h2 class="card-title">Artifacts</h2>
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
					A download delivers bytes; it does not prove the file is safe. Use the verifier CLI to
					check the signature and digest.
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
						class="file-input file-input-bordered w-full max-w-md"
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
			<section class="card card-border">
				<div class="card-body">
					<h2 class="card-title">Compatibility</h2>
					<ul class="flex flex-col gap-1 text-sm">
						{#each data.release.compatibility as entry, index (index)}
							<li>
								<span class="text-base-content/60">{entry.side}</span>
								{#if entry.loader_id}
									<Digest value={entry.loader_id} label="the loader id" length={12} />
								{/if}
								<span class="font-mono">{entry.scheme}: {entry.values.join(', ')}</span>
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
