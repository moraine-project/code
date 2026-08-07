<script lang="ts">
	import { onMount } from 'svelte';
	import { publishingMode, uploadBlob, type UploadReceipt } from '$lib/api/registry';

	let file = $state<File | null>(null);
	let receipt = $state<UploadReceipt | null>(null);
	let uploading = $state(false);
	let mode = $state('review');
	let error = $state<string | null>(null);
	let copied = $state(false);

	let projectId = $state('');
	let gameId = $state('');
	let gameVersions = $state('');
	let version = $state('');
	let channel = $state('release');
	let loader = $state('');

	onMount(async () => {
		mode = await publishingMode().catch(() => 'review');
	});

	async function upload(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const chosen = input.files?.[0];
		if (!chosen) {
			return;
		}
		file = chosen;
		uploading = true;
		error = null;
		receipt = null;
		try {
			receipt = await uploadBlob(chosen);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'the upload failed';
		} finally {
			uploading = false;
		}
	}

	function releaseCommand(): string {
		const origin = window.location.origin;
		const versions = gameVersions
			.split(',')
			.map((value) => value.trim())
			.filter((value) => value.length > 0)
			.map((value) => `--game-version ${value}`)
			.join(' ');
		const loaderFlag = loader.trim() ? ` --loader ${loader.trim()}` : '';
		return [
			'moraine-publish release --key publisher.key',
			`--home ${origin}`,
			`--project ${projectId || '<project-id>'}`,
			`--game ${gameId || '<game-id>'}`,
			versions,
			`--version ${version || '<version>'}`,
			`--channel ${channel}`,
			`--file ${file?.name ?? '<artifact>'}${loaderFlag}`,
		]
			.filter(Boolean)
			.join(' \\\n  ');
	}

	function followUpCommand(): string {
		const origin = window.location.origin;
		const project = projectId || '<project-id>';
		if (mode === 'open') {
			return [
				'moraine-publish publish --key publisher.key',
				`--home ${origin}`,
				`--project ${project}`,
				'--object <release-id>',
			].join(' \\\n  ');
		}
		return [
			'moraine-publish submit --key publisher.key',
			`--home ${origin}`,
			`--project ${project}`,
			'--object <release-id>',
			'--api-key $MORAINE_API_KEY',
		].join(' \\\n  ');
	}

	async function copy(text: string) {
		await navigator.clipboard.writeText(text);
		copied = true;
		setTimeout(() => (copied = false), 1500);
	}
</script>

<svelte:head>
	<title>Publish · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<h1 class="text-2xl font-bold">Publish</h1>
	<p class="text-base-content/80 max-w-2xl">
		This console uploads the artifact and lays out the exact command to run. Signing stays on your
		machine with <code>moraine-publish</code>: the browser never holds your release key, and only a
		key holder can publish.
	</p>

	{#if error}
		<div role="alert" class="alert alert-error"><span>{error}</span></div>
	{/if}

	<section class="card card-border">
		<div class="card-body">
			<h2 class="card-title">1. Upload the artifact</h2>
			<div class="flex items-center gap-2">
				<input
					type="file"
					class="file-input file-input-bordered w-full max-w-md"
					onchange={upload}
					disabled={uploading}
					aria-label="Artifact to upload"
				/>
				{#if uploading}
					<span class="loading loading-spinner loading-sm"></span>
				{/if}
			</div>
			{#if receipt}
				<p class="text-sm">
					Stored as <span class="font-mono">{receipt.digest}</span> ({receipt.size} bytes). The release
					must record this exact digest; <code>moraine-publish</code> computes it from the same file.
				</p>
			{/if}
		</div>
	</section>

	<section class="card card-border">
		<div class="card-body">
			<h2 class="card-title">2. Describe the release</h2>
			<div class="grid gap-3 sm:grid-cols-2">
				<label class="floating-label">
					<span>Project ID</span>
					<input class="input w-full" bind:value={projectId} placeholder="gd:sha256:…" />
				</label>
				<label class="floating-label">
					<span>Game ID</span>
					<input class="input w-full" bind:value={gameId} placeholder="gd:sha256:…" />
				</label>
				<label class="floating-label">
					<span>Game versions (comma separated)</span>
					<input class="input w-full" bind:value={gameVersions} placeholder="1.20.1" />
				</label>
				<label class="floating-label">
					<span>Version</span>
					<input class="input w-full" bind:value={version} placeholder="1.2.3" />
				</label>
				<label class="floating-label">
					<span>Channel</span>
					<input class="input w-full" bind:value={channel} />
				</label>
				<label class="floating-label">
					<span>Loader ID (optional)</span>
					<input class="input w-full" bind:value={loader} placeholder="gd:sha256:…" />
				</label>
			</div>
		</div>
	</section>

	<section class="card card-border">
		<div class="card-body">
			<h2 class="card-title">3. Sign and publish</h2>
			<p class="text-base-content/80 text-sm">
				Run the release command where the artifact and your key live. It signs the release and
				stores it at this home, then prints a release ID.
			</p>
			<div class="mockup-code text-xs">
				<pre><code>{releaseCommand()}</code></pre>
			</div>
			<button class="btn btn-sm w-fit" onclick={() => copy(releaseCommand())}>
				{copied ? 'Copied' : 'Copy command'}
			</button>
			<p class="text-base-content/80 mt-2 text-sm">
				{mode === 'open'
					? 'This home publishes directly, so append the feed entry with the release ID:'
					: 'This home reviews submissions, so submit the release ID for review:'}
			</p>
			<div class="mockup-code text-xs">
				<pre><code>{followUpCommand()}</code></pre>
			</div>
			<button class="btn btn-sm w-fit" onclick={() => copy(followUpCommand())}>
				{copied ? 'Copied' : 'Copy command'}
			</button>
		</div>
	</section>
</div>
