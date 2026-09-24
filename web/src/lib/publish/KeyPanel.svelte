<script lang="ts">
	import { generateSeed, isSeed, keyId, publicKey } from '$lib/signer';

	let {
		busy,
		seed = $bindable(''),
		keyPublic = $bindable(''),
		additionalRootSeed = $bindable(''),
	}: {
		busy: boolean;
		seed?: string;
		keyPublic?: string;
		additionalRootSeed?: string;
	} = $props();

	let keyFingerprint = $state('');
	let keyGenerated = $state(false);
	let keyMessage = $state<string | null>(null);
	let keyOpen = $state(false);

	async function applySeed(value: string) {
		keyMessage = null;
		if (!isSeed(value)) {
			seed = '';
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
</script>

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
			Your key proves a release is yours. Generate one and save the file somewhere safe, or open a
			key you already have.
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
			<label class="floating-label max-w-xl">
				<span>Additional root signing key (optional)</span>
				<input
					class="input w-full font-mono text-xs"
					type="password"
					bind:value={additionalRootSeed}
					placeholder="64 hex characters"
					aria-label="Additional root signing key"
				/>
			</label>
			<p class="text-xs text-base-content/60">
				Add a second root you control if this project may later need a two-signature ownership
				transfer.
			</p>
		{/if}
	</div>
</section>
