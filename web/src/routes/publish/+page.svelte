<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { apiOrigin } from '$lib/api/session';
	import { appendFeed, createProject, storeObject, submitFeed } from '$lib/api/publish';
	import { uploadBlob, type UploadReceipt } from '$lib/api/blobs';
	import {
		fetchGamePayload,
		listDefinitions,
		listLoaders,
		type DefinitionSummary,
		type GamePayload,
	} from '$lib/api/definitions';
	import { fetchObject, fetchProject } from '$lib/api/projects';
	import { ApiError } from '$lib/api/request';
	import { instance } from '$lib/instance.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import MultiSelectField from '$lib/components/MultiSelectField.svelte';
	import PageHeader from '$lib/components/PageHeader.svelte';
	import SelectField, { type SelectOption } from '$lib/components/SelectField.svelte';
	import { home } from '$lib/home.svelte';
	import { normalizeHome } from '$lib/home';
	import KeyPanel from '$lib/publish/KeyPanel.svelte';
	import TransferPanel from '$lib/publish/TransferPanel.svelte';
	import { session } from '$lib/session.svelte';
	import {
		genesisRoots,
		randomNonce,
		signChangelog,
		signFeedEntry,
		signGenesis,
		signModpack,
		signProfile,
		signRelease,
		signWithdrawal,
	} from '$lib/signer';
	import { pageTitle } from '$lib/title.svelte';

	let seed = $state('');
	let keyPublic = $state('');
	let additionalRootSeed = $state('');

	const mode = $derived(instance.publishing);
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

	let busy = $state(false);
	let error = $state<string | null>(null);
	let log = $state<string[]>([]);

	type PublishStage = 'idle' | 'upload' | 'sign' | 'publish';
	const MAX_OPEN_APPEND_ATTEMPTS = 3;
	const DRAFT_STORAGE_KEY = 'moraine.publish.draft.v1';
	const stageLabels: Record<PublishStage, string> = {
		idle: 'Idle',
		upload: 'Uploading',
		sign: 'Signing',
		publish: 'Publishing',
	};
	let stage = $state<PublishStage>('idle');
	let stageDetail = $state('');
	let draftReady = $state(false);
	let browserStorage: Storage | null = null;

	type PublishDraft = {
		projectMode: 'new' | 'existing';
		projectId: string;
		displayName: string;
		summary: string;
		description: string;
		gameId: string;
		version: string;
		channel: string;
		releaseKind: 'mod' | 'modpack';
		modpackManifest: string;
		notes: string;
		withdrawalRelease: string;
		withdrawalReason: string;
		withdrawalNote: string;
	};

	onMount(() => {
		try {
			browserStorage = window.localStorage;
		} catch {
			browserStorage = null;
		}
		loadDraft();
		draftReady = true;
		const save = () => saveDraft();
		document.addEventListener('input', save);
		document.addEventListener('change', save);
		void listDefinitions(normalizeHome(apiOrigin()), 'games')
			.then((loaded) => (games = loaded))
			.catch(() => {});
		return () => {
			document.removeEventListener('input', save);
			document.removeEventListener('change', save);
		};
	});

	const gameInfo = $derived.by(async () => {
		if (!gameId) {
			return { payload: null as GamePayload | null, loaders: [] as DefinitionSummary[] };
		}
		const base = normalizeHome(apiOrigin());
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
			const base = normalizeHome(apiOrigin());
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
	const pendingGame = $derived(
		gameOptions.find((option) => option.value === gameId)?.label ?? gameId,
	);
	const pendingPreview = $derived({
		project:
			displayName.trim() ||
			(projectMode === 'new' ? 'New project' : projectId.trim() || 'Project not selected'),
		projectId: projectId.trim() || 'Not created yet',
		game: pendingGame || 'Not selected',
		version: version.trim() || 'Not set',
		file: artifact?.name ?? 'No file selected',
		channel: `${releaseKind} · ${channel.trim() || 'release'}`,
	});
	const stageLabel = $derived(stageLabels[stage]);
	const projectValidation = $derived(
		projectMode === 'existing' && !projectId.trim() ? 'Enter a project ID.' : '',
	);
	const gameValidation = $derived(gameId ? '' : 'Choose a game.');
	const versionValidation = $derived(version.trim() ? '' : 'Enter a version.');
	const fileValidation = $derived(artifact ? '' : 'Choose a file to publish.');
	const gameVersionsValidation = $derived(
		gameVersions.length > 0 ? '' : 'Choose or enter a game version.',
	);
	const manifestValidation = $derived(
		releaseKind === 'modpack' && !modpackManifest.trim() ? 'Paste a signed modpack manifest.' : '',
	);
	const releaseProjectValidation = $derived(
		projectId.trim() ? '' : 'Create or enter a project first.',
	);

	function report(line: string) {
		log = [...log, line];
	}

	function fail(message: string) {
		error = message;
	}

	function draftSnapshot(): PublishDraft {
		return {
			projectMode,
			projectId,
			displayName,
			summary,
			description,
			gameId,
			version,
			channel,
			releaseKind: releaseKind === 'modpack' ? 'modpack' : 'mod',
			modpackManifest,
			notes,
			withdrawalRelease,
			withdrawalReason,
			withdrawalNote,
		};
	}

	function saveDraft() {
		if (!draftReady) return;
		try {
			browserStorage?.setItem(DRAFT_STORAGE_KEY, JSON.stringify(draftSnapshot()));
		} catch {
			return;
		}
	}

	function loadDraft() {
		try {
			const raw = browserStorage?.getItem(DRAFT_STORAGE_KEY);
			if (!raw) return;
			const draft = JSON.parse(raw) as Partial<PublishDraft> | null;
			if (!draft || typeof draft !== 'object') return;
			if (draft.projectMode === 'new' || draft.projectMode === 'existing') {
				projectMode = draft.projectMode;
			}
			if (draft.releaseKind === 'mod' || draft.releaseKind === 'modpack') {
				releaseKind = draft.releaseKind;
			}
			if (typeof draft.projectId === 'string') projectId = draft.projectId;
			if (typeof draft.displayName === 'string') displayName = draft.displayName;
			if (typeof draft.summary === 'string') summary = draft.summary;
			if (typeof draft.description === 'string') description = draft.description;
			if (typeof draft.gameId === 'string') gameId = draft.gameId;
			if (typeof draft.version === 'string') version = draft.version;
			if (typeof draft.channel === 'string') channel = draft.channel;
			if (typeof draft.modpackManifest === 'string') modpackManifest = draft.modpackManifest;
			if (typeof draft.notes === 'string') notes = draft.notes;
			if (typeof draft.withdrawalRelease === 'string') withdrawalRelease = draft.withdrawalRelease;
			if (typeof draft.withdrawalReason === 'string') withdrawalReason = draft.withdrawalReason;
			if (typeof draft.withdrawalNote === 'string') withdrawalNote = draft.withdrawalNote;
		} catch {
			return;
		}
	}

	function forgetDraft() {
		try {
			browserStorage?.removeItem(DRAFT_STORAGE_KEY);
		} catch {
			return;
		}
	}

	function clearDraft() {
		if (!draftReady) return;
		forgetDraft();
		projectMode = 'new';
		projectId = '';
		displayName = '';
		summary = '';
		description = '';
		selectedCategories = [];
		selectedTags = [];
		gameId = '';
		gameVersions = [];
		loaderId = '';
		artifact = null;
		receipt = null;
		version = '';
		channel = 'release';
		releaseKind = 'mod';
		modpackManifest = '';
		notes = '';
		withdrawalRelease = '';
		withdrawalReason = 'author-preference';
		withdrawalNote = '';
		error = null;
		report('Saved draft cleared; signing keys were kept.');
	}

	function setProjectMode(value: 'new' | 'existing') {
		projectMode = value;
		saveDraft();
	}

	function validateProject(): boolean {
		const message = projectValidation || gameValidation;
		if (message) {
			error = message;
			return false;
		}
		return true;
	}

	function validateRelease(): boolean {
		const message =
			releaseProjectValidation ||
			gameValidation ||
			versionValidation ||
			fileValidation ||
			gameVersionsValidation ||
			manifestValidation;
		if (message) {
			error = message;
			return false;
		}
		return true;
	}

	async function createNewProject() {
		busy = true;
		error = null;
		stage = 'sign';
		stageDetail = '';
		try {
			const genesis = await signGenesis(seed, {
				nonce: randomNonce(),
				authorized_kinds: ['delegation', 'release', 'profile', 'changelog', 'modpack'],
				additional_root_seeds: additionalRootSeed.trim() ? [additionalRootSeed.trim()] : [],
				created_at: Math.floor(Date.now() / 1000),
			});
			const created = await createProject(genesis.wire);
			projectId = created.project_id;
			projectMode = 'existing';
			saveDraft();
			report(`Project created: ${created.project_id}`);
			if (displayName.trim()) {
				await publishProfile();
			}
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'the project could not be created';
		} finally {
			busy = false;
			stage = 'idle';
			stageDetail = '';
		}
	}

	async function createProjectOrPublishProfile() {
		if (!validateProject()) return;
		if (projectMode === 'existing') {
			busy = true;
			error = null;
			try {
				await publishProfile();
			} catch (cause) {
				error = cause instanceof Error ? cause.message : 'the profile could not be published';
			} finally {
				busy = false;
				stage = 'idle';
				stageDetail = '';
			}
			return;
		}
		await createNewProject();
	}

	async function publishProfile() {
		stage = 'sign';
		stageDetail = '';
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

	async function signedFeedEntry(objectId: string, kind: string) {
		const project = await fetchProject(normalizeHome(apiOrigin()), projectId);
		return signFeedEntry(seed, {
			project_id: projectId,
			sequence: project.head_seq + 1,
			previous: project.head_entry ?? undefined,
			kind,
			object_digest: objectId,
			declared_at: Math.floor(Date.now() / 1000),
		});
	}

	function isFeedConflict(cause: unknown): boolean {
		return (
			(cause instanceof ApiError && cause.status === 409) ||
			(typeof cause === 'object' && cause !== null && 'status' in cause && cause.status === 409)
		);
	}

	async function publishEntry(objectId: string, kind: string) {
		if (mode === 'open') {
			for (let attempt = 0; attempt < MAX_OPEN_APPEND_ATTEMPTS; attempt += 1) {
				stage = 'sign';
				const entry = await signedFeedEntry(objectId, kind);
				stage = 'publish';
				try {
					const appended = await appendFeed(projectId, entry.wire);
					stageDetail = '';
					report(`Feed entry ${appended.seq}: ${appended.entry}`);
					return;
				} catch (cause) {
					if (!isFeedConflict(cause) || attempt === MAX_OPEN_APPEND_ATTEMPTS - 1) {
						throw cause;
					}
					const retry = attempt + 1;
					stageDetail = `Feed head changed; retry ${retry} of ${MAX_OPEN_APPEND_ATTEMPTS - 1} after refetching the project head`;
					report(
						`Feed head changed (409); retrying append ${retry}/${MAX_OPEN_APPEND_ATTEMPTS - 1} after refetching the project head`,
					);
				}
			}
		}
		if (mode === 'open') return;

		stage = 'sign';
		const entry = await signedFeedEntry(objectId, kind);
		stage = 'publish';
		const submitted = await submitFeed(entry.wire);
		stageDetail = '';
		report(
			submitted.state === 'auto-accepted'
				? `Published automatically: ${submitted.id}`
				: `Submitted for review: ${submitted.id} (${submitted.state})`,
		);
	}

	async function publishRelease() {
		if (!validateRelease()) return;
		const file = artifact;
		if (!file) return;
		busy = true;
		error = null;
		stage = 'upload';
		stageDetail = '';
		try {
			const uploaded = await uploadBlob(file);
			receipt = uploaded;
			report(`Uploaded ${file.name}: ${uploaded.size} bytes`);
			stage = 'sign';

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

			stage = 'sign';
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
				filename: file.name,
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
			forgetDraft();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'the release could not be published';
		} finally {
			busy = false;
			stage = 'idle';
			stageDetail = '';
		}
	}

	async function withdrawRelease() {
		busy = true;
		error = null;
		stage = 'sign';
		stageDetail = '';
		try {
			const withdrawal = await signWithdrawal(seed, {
				project_id: projectId,
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
			stage = 'idle';
			stageDetail = '';
		}
	}
</script>

<svelte:head>
	<title>{pageTitle('Publish')}</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<PageHeader
		title="Publish a project"
		subtitle="Sign in, pick a game, upload a file. The console signs with a key that stays on this device and never gets uploaded."
	>
		<span class="text-xs text-base-content/60"
			>Scalar fields resume locally; keys/files stay in memory.</span
		>
		<button type="button" class="btn btn-ghost btn-sm" onclick={clearDraft}>Clear draft</button>
	</PageHeader>

	{#if error}
		<div role="alert" class="alert alert-error"><span>{error}</span></div>
	{/if}

	{#if home.isForeign}
		<EmptyState
			title="Publishing stays on your own home"
			message={`This console signs and uploads through ${home.configured || 'this site'}, not through ${home.base}. Follow the link to publish there.`}
		>
			<a
				class="btn btn-primary"
				href={home.configured ? `/?home=${encodeURIComponent(home.configured)}` : '/'}
			>
				Back to {home.configured || 'your home'}
			</a>
		</EmptyState>
	{:else if !session.user}
		<EmptyState
			title="Sign in to publish"
			message="You need an account on this instance to upload files. Creating the project itself is signed by your key."
		>
			<a class="btn btn-primary" href="/account">Sign in</a>
		</EmptyState>
	{:else}
		<KeyPanel {busy} bind:seed bind:keyPublic bind:additionalRootSeed />

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
						disabled={busy || !keyPublic || !withdrawalRelease}>Publish withdrawal</button
					>
					<TransferPanel {busy} {seed} {projectId} {report} {fail} />
				</div>
			</section>
		{/if}

		<section class="card card-border bg-base-200">
			<div class="card-body gap-4">
				<h2 class="card-title">2. Project</h2>
				<div class="tabs tabs-box w-fit" aria-label="Project mode">
					<button
						type="button"
						class="tab"
						class:tab-active={projectMode === 'new'}
						aria-pressed={projectMode === 'new'}
						onclick={() => setProjectMode('new')}>New project</button
					>
					<button
						type="button"
						class="tab"
						class:tab-active={projectMode === 'existing'}
						aria-pressed={projectMode === 'existing'}
						onclick={() => setProjectMode('existing')}>Existing project</button
					>
				</div>

				{#if projectMode === 'existing'}
					<label class="floating-label max-w-xl">
						<span>Project ID</span>
						<input
							id="project-id"
							class="input w-full font-mono text-xs"
							bind:value={projectId}
							placeholder="gd:sha256:…"
							aria-invalid={Boolean(projectValidation)}
							aria-describedby="project-validation"
						/>
					</label>
				{/if}
				{#if projectValidation}
					<p id="project-validation" class="text-xs text-error">{projectValidation}</p>
				{/if}

				<div class="grid gap-3 sm:grid-cols-2">
					<label class="floating-label">
						<span>Name</span>
						<input
							id="project-name"
							class="input w-full"
							bind:value={displayName}
							placeholder="My Mod"
						/>
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
							saveDraft();
						}}
					/>
				</div>
				{#if gameValidation}
					<p id="game-validation" class="text-xs text-error">{gameValidation}</p>
				{/if}

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
								onchange={() => saveDraft()}
							/>
							<MultiSelectField
								label="Tags"
								bind:value={selectedTags}
								options={(info.payload.tags ?? []).map((tag) => ({
									value: tag.id,
									label: tag.label,
								}))}
								placeholder="None"
								onchange={() => saveDraft()}
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
					type="button"
					class="btn btn-sm w-fit"
					onclick={createProjectOrPublishProfile}
					disabled={busy || !keyPublic || !gameId || Boolean(projectValidation)}
				>
					{projectMode === 'new' ? 'Create the project' : 'Publish the profile'}
				</button>
			</div>
		</section>

		<section class="card card-border bg-base-200">
			<div class="card-body gap-4">
				<h2 class="card-title">3. Release</h2>
				{#if releaseProjectValidation}
					<p class="text-xs text-error">{releaseProjectValidation}</p>
				{/if}
				<label class="floating-label">
					<span>File</span>
					<input
						type="file"
						class="file-input w-full"
						onchange={(event) =>
							(artifact = (event.currentTarget as HTMLInputElement).files?.[0] ?? null)}
						aria-label="File to publish"
						aria-invalid={Boolean(fileValidation)}
						aria-describedby="file-validation"
					/>
				</label>
				{#if fileValidation}
					<p id="file-validation" class="text-xs text-error">{fileValidation}</p>
				{/if}
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
							onchange={() => saveDraft()}
						/>
						<SelectField
							label="Loader"
							bind:value={loaderId}
							options={info.loaders.map((loader) => ({
								value: loader.id,
								label: loader.display_name ?? loader.id,
							}))}
							placeholder="None"
							onchange={(value) => {
								loaderId = value;
								saveDraft();
							}}
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
				{#if gameVersionsValidation}
					<p class="text-xs text-error">{gameVersionsValidation}</p>
				{/if}

				<div class="grid gap-3 sm:grid-cols-2">
					<label class="floating-label">
						<span>Version</span>
						<input
							id="release-version"
							class="input w-full"
							bind:value={version}
							placeholder="1.2.3"
							aria-invalid={Boolean(versionValidation)}
							aria-describedby="version-validation"
						/>
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
				{#if versionValidation}
					<p id="version-validation" class="text-xs text-error">{versionValidation}</p>
				{/if}
				{#if releaseKind === 'modpack'}
					<label class="floating-label">
						<span>Signed modpack manifest (JSON)</span>
						<textarea
							class="textarea w-full font-mono text-xs"
							rows="8"
							bind:value={modpackManifest}
							placeholder="Paste the manifest JSON here"
							aria-label="Signed modpack manifest JSON"
							aria-invalid={Boolean(manifestValidation)}
							aria-describedby="manifest-validation"></textarea>
					</label>
					<p class="text-xs text-base-content/60">
						The manifest lists the exact signed releases and override files in this pack. Its
						project and game IDs must match this publication.
					</p>
					{#if manifestValidation}
						<p id="manifest-validation" class="text-xs text-error">{manifestValidation}</p>
					{/if}
				{/if}
				<label class="floating-label">
					<span>Release notes (optional)</span>
					<textarea class="textarea w-full" rows="3" bind:value={notes}></textarea>
				</label>

				<div class="rounded-box border border-base-300 bg-base-100 p-3">
					<div class="flex flex-wrap items-center justify-between gap-2">
						<h3 class="font-semibold">Pending publication</h3>
						<span class="badge badge-outline" aria-live="polite">Stage: {stageLabel}</span>
					</div>
					{#if stageDetail}
						<p class="mt-1 text-xs text-warning">{stageDetail}</p>
					{/if}
					<dl class="mt-3 grid grid-cols-2 gap-x-4 gap-y-1 text-sm sm:grid-cols-3">
						<div>
							<dt class="text-base-content/60">Project</dt>
							<dd class="font-medium">{pendingPreview.project}</dd>
						</div>
						<div>
							<dt class="text-base-content/60">Project ID</dt>
							<dd class="truncate font-mono text-xs">{pendingPreview.projectId}</dd>
						</div>
						<div>
							<dt class="text-base-content/60">Game</dt>
							<dd class="font-medium">{pendingPreview.game}</dd>
						</div>
						<div>
							<dt class="text-base-content/60">Version</dt>
							<dd class="font-medium">{pendingPreview.version}</dd>
						</div>
						<div>
							<dt class="text-base-content/60">File</dt>
							<dd class="truncate font-medium">{pendingPreview.file}</dd>
						</div>
						<div>
							<dt class="text-base-content/60">Channel</dt>
							<dd class="font-medium">{pendingPreview.channel}</dd>
						</div>
					</dl>
				</div>

				<button
					type="button"
					class="btn btn-primary w-fit"
					onclick={publishRelease}
					disabled={busy ||
						!keyPublic ||
						!projectId.trim() ||
						!artifact ||
						!gameId ||
						!version.trim() ||
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
