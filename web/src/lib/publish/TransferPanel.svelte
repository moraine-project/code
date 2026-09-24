<script lang="ts">
	import { transferProject } from '$lib/api/publish';
	import { session } from '$lib/session.svelte';
	import { signTransfer } from '$lib/signer';

	let {
		busy,
		seed,
		projectId,
		report,
		fail,
	}: {
		busy: boolean;
		seed: string;
		projectId: string;
		report: (line: string) => void;
		fail: (message: string) => void;
	} = $props();

	let newOwnerSeed = $state('');
	let newOwnerKind = $state<'user' | 'org'>('user');
	let newOwnerId = $state('');

	async function transfer() {
		if (!session.user?.user_id) {
			fail('sign in again before transferring ownership');
			return;
		}
		const signed = await signTransfer(seed, newOwnerSeed, {
			project_id: projectId,
			from_kind: 'user',
			from_id: session.user.user_id,
			to_kind: newOwnerKind,
			to_id: newOwnerId,
			issued_at: Math.floor(Date.now() / 1000),
		});
		const receipt = await transferProject(projectId, signed.wire);
		report(`Ownership transfer stored: ${receipt.transfer}`);
	}

	async function run() {
		try {
			await transfer();
		} catch (cause) {
			fail(cause instanceof Error ? cause.message : 'the ownership transfer could not be stored');
		}
	}
</script>

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
<input class="input" bind:value={newOwnerId} placeholder="New owner id" aria-label="New owner id" />
<button
	class="btn btn-outline w-fit"
	type="button"
	onclick={run}
	disabled={busy || !seed || !newOwnerSeed || !newOwnerId}>Store ownership transfer</button
>
