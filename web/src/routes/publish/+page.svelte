<script lang="ts">
	import { onMount } from 'svelte';
	import { apiOrigin } from '$lib/api/session';
	import { appendFeed, createProject, storeObject, submitFeed } from '$lib/api/publish';
	import {
		fetchObject,
		fetchProject,
		listDefinitions,
		normalizeBase,
		publishingMode,
		uploadBlob,
		type DefinitionSummary,
		type UploadReceipt,
	} from '$lib/api/registry';
	import {
		generateSeed,
		genesisRoots,
		isSeed,
		keyId,
		publicKey,
		randomNonce,
		signChangelog,
		signFeedEntry,
		signGenesis,
		signProfile,
		signRelease,
	} from '$lib/signer';

	let seed = $state('');
	let keyFingerprint = $state('');
	let keyPublic = $state('');
	let keyGenerated = $state(false);
	let keyMessage = $state<string | null>(null);

	let mode = $state('review');
	let games = $state<DefinitionSummary[]>([]);

	let projectMode = $state<'new' | 'existing'>('new');
	let projectId = $state('');
	let displayName = $state('');
	let summary = $state('');
	let description = $state('');

	let artifact = $state<File | null>(null);
	let receipt = $state<UploadReceipt | null>(null);
	let gameId = $state('');
	let gameVersions = $state('');
	let loaderId = $state('');
	let version = $state('');
	let channel = $state('release');
	let notes = $state('');

	let busy = $state(false);
	let error = $state<string | null>(null);
	let log = $state<string[]>([]);
	const rootCheck = $derived.by(async () => {
		if (!projectId.startsWith('gd:sha256:') || !keyPublic) {
			return false;
		}
		try {
			const base = normalizeBase(apiOrigin());
			const project = await fetchProject(base, projectId);
			const wire = await fetchObject(base, project.genesis);
			const roots = await genesisRoots(wire);
			return roots.some((root) => root.toLowerCase() === keyPublic.toLowerCase());
		} catch {
			return false;
		}
	});

	onMount(async () => {
		mode = await publishingMode().catch(() => 'review');
		try {
			games = await listDefinitions(normalizeBase(apiOrigin()), 'games');
		} catch {
			games = [];
		}
	});

	function report(line: string) {
		log = [...log, line];
	}

	async function applySeed(value: string) {
		keyMessage = null;
		if (!isSeed(value)) {
			keyFingerprint = '';
			keyPublic = '';
			return;
		}
		seed = value.trim();
		keyFingerprint = await keyId(seed);
		keyPublic = await publicKey(seed);
	}

	async function generate() {
		await applySeed(generateSeed());
		keyGenerated = true;
	}

	async function uploadKey(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		if (!file) {
			return;
		}
		await applySeed(await file.text());
		keyGenerated = false;
	}

	function downloadKey() {
		const blob = new Blob([`${seed}\n`], { type: 'text/plain' });
		const url = URL.createObjectURL(blob);
		const anchor = document.createElement('a');
		anchor.href = url;
		anchor.download = 'publisher.key';
		anchor.click();
		URL.revokeObjectURL(url);
		keyMessage = 'Saved. Keep this file safe; anyone with it can publish as you.';
	}

	function versions(): string[] {
		return gameVersions
			.split(',')
			.map((value) => value.trim())
			.filter((value) => value.length > 0);
	}

	async function createNewProject() {
		busy = true;
		error = null;
		try {
			const genesis = await signGenesis(seed, {
				nonce: randomNonce(),
				authorized_kinds: ['delegation', 'release', 'profile', 'changelog'],
				created_at: Math.floor(Date.now() / 1000),
			});
			const created = await createProject(genesis.wire);
			projectId = created.project_id;
			projectMode = 'existing';
			report(`Project created: ${created.project_id}`);
			if (displayName.trim()) {
				await publishProfile();
			}
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'the project could not be created';
		} finally {
			busy = false;
		}
	}

	async function publishProfile() {
		const profile = await signProfile(seed, {
			project_id: projectId,
			game_id: gameId,
			nonce: randomNonce(),
			display_name: displayName,
			summary,
			description,
			declared_time: Math.floor(Date.now() / 1000),
		});
		await storeObject(projectId, 'profile', profile.wire);
		await publishEntry(profile.id, 'profile-updated');
		report(`Profile published: ${profile.id}`);
	}

	async function publishEntry(objectId: string, kind: string) {
		const project = await fetchProject(normalizeBase(apiOrigin()), projectId);
		const entry = await signFeedEntry(seed, {
			project_id: projectId,
			sequence: project.head_seq + 1,
			previous: project.head_entry ?? undefined,
			kind,
			object_digest: objectId,
			declared_at: Math.floor(Date.now() / 1000),
		});
		if (mode === 'open') {
			const appended = await appendFeed(projectId, entry.wire);
			report(`Feed entry ${appended.seq}: ${appended.entry}`);
		} else {
			const submitted = await submitFeed(entry.wire);
			report(`Submitted for review: ${submitted.id} (${submitted.state})`);
		}
	}

	async function publishRelease() {
		if (!artifact) {
			error = 'Choose an artifact first.';
			return;
		}
		busy = true;
		error = null;
		try {
			const uploaded = await uploadBlob(artifact);
			receipt = uploaded;
			report(`Uploaded artifact: sha256:${uploaded.digest} (${uploaded.size} bytes)`);

			let changelogId: string | undefined;
			if (notes.trim()) {
				const changelog = await signChangelog(seed, {
					project_id: projectId,
					locale: 'en',
					body: notes,
					declared_time: Math.floor(Date.now() / 1000),
				});
				await storeObject(projectId, 'changelog', changelog.wire);
				changelogId = changelog.id;
				report(`Changelog published: ${changelog.id}`);
			}

			const release = await signRelease(seed, {
				project_id: projectId,
				game_id: gameId,
				nonce: randomNonce(),
				human_version: version,
				channel,
				declared_time: Math.floor(Date.now() / 1000),
				game_versions: versions(),
				loader_id: loaderId.trim() || undefined,
				digest: uploaded.digest,
				size: uploaded.size,
				filename: artifact.name,
				changelog_digest: changelogId,
			});
			await storeObject(projectId, 'release', release.wire);
			report(`Release signed: ${release.id}`);
			await publishEntry(release.id, 'release-published');
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'the release could not be published';
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head>
	<title>Publish · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<h1 class="text-2xl font-bold">Publish</h1>
	<p class="text-base-content/80 max-w-2xl">
		This console signs with a key that stays in your browser. The key is never uploaded, stored, or
		sent anywhere; signing happens locally in WebAssembly using the same code as the
		<code>moraine-publish</code> CLI, so the bytes are identical.
	</p>

	{#if error}
		<div role="alert" class="alert alert-error"><span>{error}</span></div>
	{/if}

	<section class="card card-border">
		<div class="card-body gap-4">
			<h2 class="card-title">1. Your key</h2>
			<p class="text-base-content/80 text-sm max-w-2xl">
				A key is a 32-byte seed in a text file, the same file the CLI uses. Generate one here, paste
				one, or open an existing file. If you generate it, download it before you publish: losing it
				loses the project. For anything valuable, prefer a key made with the CLI on a machine you
				trust, or publish with a delegated release key rather than the project root.
			</p>
			<div class="flex flex-wrap items-center gap-2">
				<button class="btn btn-sm" onclick={generate} disabled={busy}>Generate a key</button>
				<input
					type="file"
					class="file-input file-input-sm w-full max-w-xs"
					onchange={uploadKey}
					disabled={busy}
					aria-label="Open a key file"
				/>
				<button class="btn btn-sm btn-outline" onclick={downloadKey} disabled={!seed}>
					Download the key
				</button>
			</div>
			<label class="floating-label max-w-xl">
				<span>Or paste a key (hex)</span>
				<input
					class="input w-full font-mono text-xs"
					value={seed}
					oninput={(event) => applySeed(event.currentTarget.value)}
					placeholder="64 hex characters"
				/>
			</label>
			{#if keyFingerprint}
				<div class="text-sm">
					<p>
						Key ID: <span class="font-mono">{keyFingerprint}</span>
					</p>
					<p class="text-base-content/70 break-all font-mono text-xs">{keyPublic}</p>
					{#if keyGenerated}
						<p class="text-warning mt-1">
							This key only exists in this tab. Download it now, and do not publish a project you
							care about until you have a backup.
						</p>
					{/if}
				</div>
			{/if}
			{#if keyMessage}
				<p class="text-success text-sm">{keyMessage}</p>
			{/if}
		</div>
	</section>

	<section class="card card-border">
		<div class="card-body gap-4">
			<h2 class="card-title">2. The project</h2>
			<div class="flex flex-wrap gap-2">
				<button
					class="btn btn-sm"
					class:btn-active={projectMode === 'new'}
					onclick={() => (projectMode = 'new')}>New project</button
				>
				<button
					class="btn btn-sm"
					class:btn-active={projectMode === 'existing'}
					onclick={() => (projectMode = 'existing')}>Existing project</button
				>
			</div>
			{#if projectMode === 'existing'}
				<label class="floating-label max-w-xl">
					<span>Project ID</span>
					<input
						class="input w-full font-mono text-xs"
						bind:value={projectId}
						placeholder="gd:sha256:…"
					/>
				</label>
			{:else}
				<div class="grid gap-3 sm:grid-cols-2">
					<label class="floating-label">
						<span>Display name</span>
						<input class="input w-full" bind:value={displayName} placeholder="My Mod" />
					</label>
					<label class="floating-label">
						<span>Summary</span>
						<input class="input w-full" bind:value={summary} placeholder="What it does" />
					</label>
				</div>
				<label class="floating-label">
					<span>Description</span>
					<textarea class="textarea w-full" rows="3" bind:value={description}></textarea>
				</label>
			{/if}
			{#if projectId}
				<p class="text-base-content/70 font-mono text-xs">{projectId}</p>
			{/if}
			{#await rootCheck then isRoot}
				{#if isRoot}
					<div role="alert" class="alert alert-warning">
						<span>
							This key is a root of the project. Signing here works, but the page that serves this
							console is in your trust path; for a root you care about, prefer the CLI on a machine
							you control, or delegate a release key and keep the root offline.
						</span>
					</div>
				{/if}
			{/await}
			<button
				class="btn btn-sm w-fit"
				onclick={createNewProject}
				disabled={busy || !seed || !gameId}
			>
				{projectMode === 'new' ? 'Create the project' : 'Publish the profile'}
			</button>
			<p class="text-base-content/60 text-xs">
				Creating a project signs its genesis, which fixes its identity. It cannot be renamed or
				moved later, and the ID never changes even if you publish it to another home.
			</p>
		</div>
	</section>

	<section class="card card-border">
		<div class="card-body gap-4">
			<h2 class="card-title">3. The release</h2>
			<div class="grid gap-3 sm:grid-cols-2">
				<label class="floating-label">
					<span>Game</span>
					<select class="select w-full" bind:value={gameId}>
						<option value="">Choose a game</option>
						{#each games as game (game.id)}
							<option value={game.id}>{game.display_name ?? game.id}</option>
						{/each}
					</select>
				</label>
				<label class="floating-label">
					<span>Game versions (comma separated)</span>
					<input class="input w-full" bind:value={gameVersions} placeholder="1.20.1" />
				</label>
				<label class="floating-label">
					<span>Loader (optional)</span>
					<input
						class="input w-full font-mono text-xs"
						bind:value={loaderId}
						placeholder="gd:sha256:…"
					/>
				</label>
				<label class="floating-label">
					<span>Version</span>
					<input class="input w-full" bind:value={version} placeholder="1.2.3" />
				</label>
				<label class="floating-label">
					<span>Channel</span>
					<input class="input w-full" bind:value={channel} />
				</label>
			</div>
			<label class="floating-label">
				<span>Artifact</span>
				<input
					type="file"
					class="file-input w-full"
					onchange={(event) =>
						(artifact = (event.currentTarget as HTMLInputElement).files?.[0] ?? null)}
					aria-label="Artifact to publish"
				/>
			</label>
			<label class="floating-label">
				<span>Release notes (optional, markdown)</span>
				<textarea class="textarea w-full" rows="3" bind:value={notes}></textarea>
			</label>
			{#if receipt}
				<p class="text-sm">
					Stored as <span class="font-mono">{receipt.digest}</span> ({receipt.size} bytes).
				</p>
			{/if}
			<button
				class="btn btn-primary btn-sm w-fit"
				onclick={publishRelease}
				disabled={busy || !seed || !projectId || !artifact || !gameId || !version}
			>
				{busy ? 'Working…' : 'Sign and publish'}
			</button>
			<p class="text-base-content/60 text-xs">
				{mode === 'open'
					? 'This home publishes directly, so the signed release is added to the feed.'
					: 'This home reviews submissions, so the signed release is submitted for review.'}
			</p>
		</div>
	</section>

	{#if log.length}
		<section class="card card-border">
			<div class="card-body">
				<h2 class="card-title">What happened</h2>
				<ul class="font-mono text-xs">
					{#each log as line, index (index)}
						<li>{line}</li>
					{/each}
				</ul>
			</div>
		</section>
	{/if}
</div>
