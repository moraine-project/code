<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { apiOrigin } from '$lib/api/session';
	import {
		appendFeed,
		createProject,
		storeObject,
		submitFeed,
		transferProject,
	} from '$lib/api/publish';
	import {
		fetchGamePayload,
		fetchObject,
		fetchProject,
		listDefinitions,
		listLoaders,
		normalizeBase,
		publishingMode,
		uploadBlob,
		type DefinitionSummary,
		type GamePayload,
		type UploadReceipt,
	} from '$lib/api/registry';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import MultiSelectField from '$lib/components/MultiSelectField.svelte';
	import PageHeader from '$lib/components/PageHeader.svelte';
	import SelectField, { type SelectOption } from '$lib/components/SelectField.svelte';
	import { session } from '$lib/session.svelte';
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
		signModpack,
		signProfile,
		signRelease,
		signTransfer,
		signWithdrawal,
	} from '$lib/signer';

	let seed = $state('');
	let keyFingerprint = $state('');
	let keyPublic = $state('');
	let keyGenerated = $state(false);
	let keyMessage = $state<string | null>(null);
	let keyOpen = $state(false);

	let mode = $state('review');
	let games = $state<DefinitionSummary[]>([]);

	let projectMode = $state<'new' | 'existing'>('new');
	let projectId = $state(page.url.searchParams.get('project') ?? '');
	let displayName = $state('');
	let summary = $state('');
	let description = $state('');
	let selectedCategories = $state<string[]>([]);
	let selectedTags = $state<string[]>([]);

	let gameId = $state('');
	let artifact = $state<File | null>(null);
	let receipt = $state<UploadReceipt | null>(null);
	let gameVersions = $state<string[]>([]);
	let loaderId = $state('');
	let version = $state('');
	let channel = $state('release');
	let releaseKind = $state('mod');
	let modpackManifest = $state('');
	let notes = $state('');
	let withdrawalRelease = $state('');
	let withdrawalReason = $state('author-preference');
	let withdrawalNote = $state('');
	let newOwnerSeed = $state('');
	let newOwnerKind = $state('user');
	let newOwnerId = $state('');

	let busy = $state(false);
	let error = $state<string | null>(null);
	let log = $state<string[]>([]);

	onMount(async () => {
		mode = await publishingMode().catch(() => 'review');
		games = await listDefinitions(normalizeBase(apiOrigin()), 'games').catch(() => []);
	});

	const gameInfo = $derived.by(async () => {
		if (!gameId) {
			return { payload: null as GamePayload | null, loaders: [] as DefinitionSummary[] };
		}
		const base = normalizeBase(apiOrigin());
		const [payload, loaders] = await Promise.all([
			fetchGamePayload(base, gameId).catch(() => null),
			listLoaders(base, gameId).catch(() => []),
		]);
		return { payload, loaders };
	});

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

	const gameOptions = $derived<SelectOption[]>(
		games.map((game) => ({ value: game.id, label: game.display_name ?? game.id })),
	);

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
		keyOpen = true;
	}

	async function uploadKey(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		if (file) {
			await applySeed(await file.text());
			keyGenerated = false;
		}
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

	async function createNewProject() {
		busy = true;
		error = null;
		try {
			const genesis = await signGenesis(seed, {
				nonce: randomNonce(),
				authorized_kinds: ['delegation', 'release', 'profile', 'changelog', 'modpack'],
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

	async function createProjectOrPublishProfile() {
		if (projectMode === 'existing') {
			if (!projectId.trim()) {
				error = 'Enter a project ID first.';
				return;
			}
			busy = true;
			error = null;
			try {
				await publishProfile();
			} catch (cause) {
				error = cause instanceof Error ? cause.message : 'the profile could not be published';
			} finally {
				busy = false;
			}
			return;
		}
		await createNewProject();
	}

	async function publishProfile() {
		const profile = await signProfile(seed, {
			project_id: projectId,
			game_id: gameId,
			nonce: randomNonce(),
			display_name: displayName,
			summary,
			description,
			categories: selectedCategories,
			tags: selectedTags,
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
			report(
				submitted.state === 'auto-accepted'
					? `Published automatically: ${submitted.id}`
					: `Submitted for review: ${submitted.id} (${submitted.state})`,
			);
		}
	}

	async function publishRelease() {
		if (!artifact) {
			error = 'Choose a file first.';
			return;
		}
		busy = true;
		error = null;
		try {
			const uploaded = await uploadBlob(artifact);
			receipt = uploaded;
			report(`Uploaded ${artifact.name}: ${uploaded.size} bytes`);

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

			if (releaseKind === 'modpack') {
				const manifest = JSON.parse(modpackManifest) as { project_id?: string; game_id?: string };
				if (manifest.project_id !== projectId || manifest.game_id !== gameId) {
					throw new Error('the modpack manifest project and game must match this publication');
				}
			}

			const release = await signRelease(seed, {
				project_id: projectId,
				game_id: gameId,
				nonce: randomNonce(),
				human_version: version,
				channel,
				kind: releaseKind,
				declared_time: Math.floor(Date.now() / 1000),
				game_versions: gameVersions,
				loader_id: loaderId || undefined,
				digest: uploaded.digest,
				size: uploaded.size,
				filename: artifact.name,
				changelog_digest: changelogId,
			});
			if (releaseKind === 'modpack') {
				const manifest = await signModpack(seed, JSON.parse(modpackManifest));
				await storeObject(projectId, 'modpack', manifest.wire);
				report(`Modpack manifest signed: ${manifest.id}`);
				await publishEntry(manifest.id, 'modpack-published');
			}
			await storeObject(projectId, 'release', release.wire);
			report(`Release signed: ${release.id}`);
			await publishEntry(release.id, 'release-published');
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'the release could not be published';
		} finally {
			busy = false;
		}
	}

	async function withdrawRelease() {
		busy = true;
		error = null;
		try {
			const withdrawal = await signWithdrawal(seed, {
				release_id: withdrawalRelease,
				reason: withdrawalReason,
				note: withdrawalNote || undefined,
				declared_time: Math.floor(Date.now() / 1000),
			});
			await storeObject(projectId, 'release', withdrawal.wire);
			await publishEntry(withdrawal.id, 'release-withdrawn');
			report(`Withdrawal published: ${withdrawal.id}`);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'the withdrawal could not be published';
		} finally {
			busy = false;
		}
	}

	async function transferOwnership() {
		busy = true;
		error = null;
		try {
			if (!session.user?.user_id) throw new Error('sign in again before transferring ownership');
			const transfer = await signTransfer(seed, newOwnerSeed, {
				project_id: projectId,
				from_kind: 'user',
				from_id: session.user.user_id,
				to_kind: newOwnerKind,
				to_id: newOwnerId,
				issued_at: Math.floor(Date.now() / 1000),
			});
			const transferReceipt = await transferProject(projectId, transfer.wire);
			report(`Ownership transfer stored: ${transferReceipt.transfer}`);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'the ownership transfer could not be stored';
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head>
	<title>Publish · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<PageHeader
		title="Publish a project"
		subtitle="Sign in, pick a game, upload a file. The console signs with a key that stays on this device and never gets uploaded."
	/>

	{#if error}
		<div role="alert" class="alert alert-error"><span>{error}</span></div>
	{/if}

	{#if !session.user}
		<EmptyState
			title="Sign in to publish"
			message="You need an account on this instance to upload files. Creating the project itself is signed by your key."
		>
			<a class="btn btn-primary" href="/account">Sign in</a>
		</EmptyState>
	{:else}
		<section class="card card-border bg-base-200">
			<div class="card-body gap-4">
				<div class="flex flex-wrap items-center justify-between gap-2">
					<h2 class="card-title">1. Signing key</h2>
					{#if keyFingerprint}
						<span class="badge badge-success badge-sm">Key ready</span>
					{:else}
						<span class="badge badge-warning badge-sm">Key needed</span>
					{/if}
				</div>
				<p class="text-sm text-base-content/70">
					Your key proves a release is yours. Generate one and save the file somewhere safe, or open
					a key you already have.
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
					{#if seed}
						<button class="btn btn-sm btn-outline" onclick={downloadKey}>Download the key</button>
					{/if}
					<button class="btn btn-ghost btn-sm" onclick={() => (keyOpen = !keyOpen)}>
						{keyOpen ? 'Hide' : 'Advanced'}
					</button>
				</div>
				{#if keyFingerprint}
					<p class="text-sm">
						Key ID: <span class="font-mono text-xs">{keyFingerprint}</span>
					</p>
				{/if}
				{#if keyMessage}
					<p class="text-sm text-success">{keyMessage}</p>
				{/if}
				{#if keyOpen}
					<label class="floating-label max-w-xl">
						<span>Paste a key (hex)</span>
						<input
							class="input w-full font-mono text-xs"
							value={seed}
							oninput={(event) => applySeed(event.currentTarget.value)}
							placeholder="64 hex characters"
						/>
					</label>
					{#if keyGenerated}
						<p class="text-sm text-warning">
							This key exists only in this tab until you download it. Do that before you publish.
						</p>
					{/if}
				{/if}
			</div>
		</section>

		{#if projectMode === 'existing' && projectId}
			<section class="card card-border bg-base-200">
				<div class="card-body gap-4">
					<h2 class="card-title">Project lifecycle</h2>
					<p class="text-sm text-base-content/70">
						These actions create signed records. Save the key before using them.
					</p>
					<div class="grid gap-3 sm:grid-cols-2">
						<input
							class="input"
							bind:value={withdrawalRelease}
							placeholder="Release object id"
							aria-label="Release object id"
						/>
						<select class="select" bind:value={withdrawalReason} aria-label="Withdrawal reason"
							><option>author-preference</option><option>broken</option><option>compromise</option
							><option>harmful</option><option>legal</option></select
						>
					</div>
					<input
						class="input"
						bind:value={withdrawalNote}
						placeholder="Withdrawal note (optional)"
						aria-label="Withdrawal note"
					/>
					<button
						class="btn btn-outline w-fit"
						type="button"
						onclick={withdrawRelease}
						disabled={busy || !seed || !withdrawalRelease}>Publish withdrawal</button
					>
					<div class="divider my-1"></div>
					<h3 class="font-semibold">Transfer ownership</h3>
					<div class="grid gap-3 sm:grid-cols-2">
						<input
							class="input"
							type="password"
							bind:value={newOwnerSeed}
							placeholder="New owner signing key"
							aria-label="New owner signing key"
						/>
						<select class="select" bind:value={newOwnerKind} aria-label="New owner kind"
							><option value="user">User</option><option value="org">Organization</option></select
						>
					</div>
					<input
						class="input"
						bind:value={newOwnerId}
						placeholder="New owner id"
						aria-label="New owner id"
					/>
					<button
						class="btn btn-outline w-fit"
						type="button"
						onclick={transferOwnership}
						disabled={busy || !seed || !newOwnerSeed || !newOwnerId}
						>Store ownership transfer</button
					>
				</div>
			</section>
		{/if}

		<section class="card card-border bg-base-200">
			<div class="card-body gap-4">
				<h2 class="card-title">2. Project</h2>
				<div role="tablist" class="tabs tabs-box w-fit">
					<button
						role="tab"
						class="tab"
						class:tab-active={projectMode === 'new'}
						onclick={() => (projectMode = 'new')}>New project</button
					>
					<button
						role="tab"
						class="tab"
						class:tab-active={projectMode === 'existing'}
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
				{/if}

				<div class="grid gap-3 sm:grid-cols-2">
					<label class="floating-label">
						<span>Name</span>
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

				<div class="max-w-xl">
					<SelectField
						label="Game"
						bind:value={gameId}
						options={gameOptions}
						placeholder="Choose a game"
						onchange={(value) => {
							gameId = value;
							selectedCategories = [];
							selectedTags = [];
							gameVersions = [];
							loaderId = '';
						}}
					/>
				</div>

				{#await gameInfo then info}
					{#if info.payload && ((info.payload.categories?.length ?? 0) > 0 || (info.payload.tags?.length ?? 0) > 0)}
						<div class="grid gap-3 sm:grid-cols-2">
							<MultiSelectField
								label="Categories"
								bind:value={selectedCategories}
								options={(info.payload.categories ?? []).map((category) => ({
									value: category.id,
									label: category.label,
								}))}
								placeholder="None"
							/>
							<MultiSelectField
								label="Tags"
								bind:value={selectedTags}
								options={(info.payload.tags ?? []).map((tag) => ({
									value: tag.id,
									label: tag.label,
								}))}
								placeholder="None"
							/>
						</div>
					{/if}
				{/await}

				{#if projectId}
					<p class="font-mono text-xs text-base-content/60">{projectId}</p>
				{/if}
				{#await rootCheck then isRoot}
					{#if isRoot}
						<div role="alert" class="alert alert-warning">
							<span>
								This is the project's root key. That works, but the page serving this console is in
								your trust path. For a root you care about, use the CLI on a machine you control.
							</span>
						</div>
					{/if}
				{/await}
				<button
					class="btn btn-sm w-fit"
					onclick={createProjectOrPublishProfile}
					disabled={busy || !seed || !gameId}
				>
					{projectMode === 'new' ? 'Create the project' : 'Publish the profile'}
				</button>
			</div>
		</section>

		<section class="card card-border bg-base-200">
			<div class="card-body gap-4">
				<h2 class="card-title">3. Release</h2>
				<label class="floating-label">
					<span>File</span>
					<input
						type="file"
						class="file-input w-full"
						onchange={(event) =>
							(artifact = (event.currentTarget as HTMLInputElement).files?.[0] ?? null)}
						aria-label="File to publish"
					/>
				</label>
				{#if receipt}
					<p class="text-sm text-base-content/70">
						Uploaded: <span class="font-mono">{receipt.size}</span> bytes
					</p>
				{/if}

				{#await gameInfo then info}
					<div class="grid gap-3 sm:grid-cols-2">
						<MultiSelectField
							label="Game versions"
							bind:value={gameVersions}
							options={(info.payload?.version_catalog ?? []).map((value) => ({
								value,
								label: value,
							}))}
							placeholder="Any version"
							hint={info.payload?.version_catalog?.length
								? ''
								: 'Type versions by hand below if the game has no list.'}
						/>
						<SelectField
							label="Loader"
							bind:value={loaderId}
							options={info.loaders.map((loader) => ({
								value: loader.id,
								label: loader.display_name ?? loader.id,
							}))}
							placeholder="None"
						/>
					</div>
				{/await}

				{#if gameVersions.length === 0}
					<label class="floating-label max-w-xl">
						<span>Game versions (comma separated)</span>
						<input
							class="input w-full"
							placeholder="1.20.1"
							oninput={(event) =>
								(gameVersions = event.currentTarget.value
									.split(',')
									.map((value) => value.trim())
									.filter(Boolean))}
						/>
					</label>
				{/if}

				<div class="grid gap-3 sm:grid-cols-2">
					<label class="floating-label">
						<span>Version</span>
						<input class="input w-full" bind:value={version} placeholder="1.2.3" />
					</label>
					<label class="floating-label">
						<span>Channel</span>
						<input class="input w-full" bind:value={channel} />
					</label>
					<label class="floating-label">
						<span>Artifact kind</span>
						<select class="select w-full" bind:value={releaseKind} aria-label="Artifact kind">
							<option value="mod">Mod</option>
							<option value="modpack">Modpack</option>
						</select>
					</label>
				</div>
				{#if releaseKind === 'modpack'}
					<label class="floating-label">
						<span>Signed modpack manifest (JSON)</span>
						<textarea
							class="textarea w-full font-mono text-xs"
							rows="8"
							bind:value={modpackManifest}
							placeholder="Paste the manifest JSON here"
							aria-label="Signed modpack manifest JSON"></textarea>
					</label>
					<p class="text-xs text-base-content/60">
						The manifest lists the exact signed releases and override files in this pack. Its
						project and game IDs must match this publication.
					</p>
				{/if}
				<label class="floating-label">
					<span>Release notes (optional)</span>
					<textarea class="textarea w-full" rows="3" bind:value={notes}></textarea>
				</label>

				<button
					class="btn btn-primary w-fit"
					onclick={publishRelease}
					disabled={busy ||
						!seed ||
						!projectId ||
						!artifact ||
						!gameId ||
						!version ||
						gameVersions.length === 0 ||
						(releaseKind === 'modpack' && !modpackManifest.trim())}
				>
					{busy ? 'Publishing…' : 'Publish release'}
				</button>
				<p class="text-xs text-base-content/60">
					{mode === 'open'
						? 'This instance publishes directly.'
						: mode === 'progressive'
							? 'The first release is reviewed; later routine releases may publish automatically.'
							: 'This instance reviews submissions before they appear.'}
				</p>
			</div>
		</section>

		{#if log.length}
			<section class="card card-border bg-base-200">
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
	{/if}
</div>
