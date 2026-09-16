<script lang="ts">
	import { onMount } from 'svelte';
	import { KeyRound, Plus, Trash2, UserPlus } from '@lucide/svelte';
	import {
		accounts,
		createAccount,
		deleteAccount,
		resetPassword,
		type AdminAccount,
	} from '$lib/api/admin';
	import Avatar from '$lib/components/Avatar.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import SelectField, { type SelectOption } from '$lib/components/SelectField.svelte';
	import { session } from '$lib/session.svelte';

	let people = $state<AdminAccount[]>([]);
	let email = $state('');
	let role = $state('member');
	let busy = $state(false);
	let error = $state<string | null>(null);
	let notice = $state<string | null>(null);

	const roleOptions: SelectOption[] = [
		{ value: 'member', label: 'Member' },
		{ value: 'operator', label: 'Operator' },
	];

	onMount(load);

	async function load() {
		try {
			people = await accounts();
			error = null;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not load accounts';
		}
	}

	async function create(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		error = null;
		notice = null;
		try {
			const created = await createAccount(email.trim(), role);
			notice = `Created ${email.trim()}. Temporary password: ${created.password} — share it once, then have them change it.`;
			email = '';
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not create the account';
		} finally {
			busy = false;
		}
	}

	async function reset(account: AdminAccount) {
		busy = true;
		error = null;
		notice = null;
		try {
			const password = await resetPassword(account.email);
			notice = `Temporary password for ${account.email}: ${password}`;
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not reset the password';
		} finally {
			busy = false;
		}
	}

	async function remove(account: AdminAccount) {
		busy = true;
		error = null;
		notice = null;
		try {
			await deleteAccount(account.user_id);
			notice = `Deleted ${account.email}.`;
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not delete the account';
		} finally {
			busy = false;
		}
	}

	function formatTime(seconds: number): string {
		return new Date(seconds * 1000).toLocaleDateString();
	}
</script>

<div class="flex flex-col gap-4">
	{#if error}
		<div role="alert" class="alert alert-error"><span>{error}</span></div>
	{/if}
	{#if notice}
		<div role="alert" class="alert alert-info"><span class="font-mono text-xs">{notice}</span></div>
	{/if}

	<form class="flex flex-wrap items-end gap-3" onsubmit={create}>
		<label class="floating-label">
			<span>Email</span>
			<input class="input w-64" type="email" bind:value={email} required />
		</label>
		<div class="w-36">
			<SelectField label="Role" bind:value={role} options={roleOptions} />
		</div>
		<button class="btn" type="submit" disabled={busy}>
			<UserPlus size={16} />
			Create account
		</button>
	</form>
	<p class="text-xs text-base-content/50">
		Creating an account returns a temporary password shown once. Operators can manage federation,
		review, and directory policy; members can publish.
	</p>

	{#if people.length === 0}
		<EmptyState title="No accounts" message="Create one above." />
	{:else}
		<ul class="flex flex-col divide-y divide-base-300">
			{#each people as account (account.user_id)}
				<li class="flex flex-wrap items-center gap-3 py-2">
					<Avatar name={account.email} id={account.user_id} size={34} />
					<span class="min-w-0 flex-1 truncate">
						{account.email}
						{#if account.user_id === session.user?.user_id}
							<span class="badge badge-ghost badge-sm ml-1">you</span>
						{/if}
					</span>
					<span class="badge badge-outline badge-sm">{account.role}</span>
					{#if !account.verified}
						<span class="badge badge-warning badge-sm">unverified</span>
					{/if}
					<span class="text-xs text-base-content/50">{formatTime(account.created_at)}</span>
					<button class="btn btn-ghost btn-xs" onclick={() => reset(account)} disabled={busy}>
						<KeyRound size={13} />
						reset
					</button>
					{#if account.user_id !== session.user?.user_id}
						<button class="btn btn-ghost btn-xs" onclick={() => remove(account)} disabled={busy}>
							<Trash2 size={13} />
							delete
						</button>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</div>
