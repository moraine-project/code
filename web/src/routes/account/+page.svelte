<script lang="ts">
	import { onMount } from 'svelte';
	import { KeyRound, ShieldCheck } from '@lucide/svelte';
	import {
		changePassword,
		issueRecoveryCodes,
		login,
		logout,
		recover,
		register,
	} from '$lib/api/session';
	import { registrationMode } from '$lib/api/registry';
	import Avatar from '$lib/components/Avatar.svelte';
	import PageHeader from '$lib/components/PageHeader.svelte';
	import { session } from '$lib/session.svelte';

	let user = $state<typeof session.user>(null);
	let email = $state('');
	let password = $state('');
	let creating = $state(false);
	let recovering = $state(false);
	let busy = $state(false);
	let error = $state<string | null>(null);
	let notice = $state<string | null>(null);
	let registrationOpen = $state(true);

	let current = $state('');
	let next = $state('');
	let codes = $state<string[]>([]);
	let recoveryEmail = $state('');
	let recoveryCode = $state('');
	let recoveryNext = $state('');

	onMount(() => {
		void refresh();
		void registrationMode()
			.then((mode) => (registrationOpen = mode === 'open'))
			.catch(() => (registrationOpen = false));
	});

	async function refresh() {
		user = await session.refresh();
	}

	function clear() {
		error = null;
		notice = null;
	}

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		clear();
		busy = true;
		try {
			if (creating) {
				await register(email, password);
				notice = 'Account created. Sign in to continue.';
				creating = false;
			} else {
				await login(email, password);
				await refresh();
			}
			password = '';
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'the request failed';
		} finally {
			busy = false;
		}
	}

	async function submitRecovery(event: SubmitEvent) {
		event.preventDefault();
		clear();
		busy = true;
		try {
			await recover(recoveryEmail, recoveryCode, recoveryNext);
			notice = 'Password changed. Sign in with the new one.';
			recovering = false;
			recoveryCode = '';
			recoveryNext = '';
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'the request failed';
		} finally {
			busy = false;
		}
	}

	async function submitChange(event: SubmitEvent) {
		event.preventDefault();
		clear();
		busy = true;
		try {
			await changePassword(current, next);
			notice = 'Password changed. Other sessions were signed out.';
			current = '';
			next = '';
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'the request failed';
		} finally {
			busy = false;
		}
	}

	async function showCodes() {
		clear();
		busy = true;
		try {
			codes = await issueRecoveryCodes();
			notice = 'Save these now. Each works once and is not shown again.';
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'the request failed';
		} finally {
			busy = false;
		}
	}

	async function signOut() {
		clear();
		busy = true;
		try {
			await logout();
			session.set(null);
			user = null;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'sign out failed';
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head>
	<title>Account · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<PageHeader title="Account" subtitle="Sign in to publish, submit, and manage what you follow." />

	{#if error}
		<div role="alert" class="alert alert-error"><span>{error}</span></div>
	{/if}
	{#if notice}
		<div role="alert" class="alert alert-info"><span>{notice}</span></div>
	{/if}

	{#if user}
		<section class="card card-border max-w-xl bg-base-200">
			<div class="card-body gap-4">
				<div class="flex items-center gap-4">
					<Avatar name={user.email} id={user.user_id} size={56} />
					<div>
						<h2 class="text-lg font-semibold">{user.email}</h2>
						<p class="text-sm text-base-content/60">Signed in by {user.via}</p>
					</div>
				</div>
				<div class="flex flex-wrap gap-2">
					<a class="btn btn-sm" href="/publish">Publish a mod</a>
					<a class="btn btn-sm btn-outline" href="/submissions">Submissions</a>
					<a class="btn btn-sm btn-outline" href="/orgs">Organizations</a>
					<a class="btn btn-sm btn-outline" href="/review">Review queue</a>
					<button class="btn btn-sm btn-ghost" onclick={signOut} disabled={busy}>Sign out</button>
				</div>
			</div>
		</section>

		<section class="card card-border max-w-xl bg-base-200">
			<div class="card-body gap-4">
				<h2 class="card-title"><KeyRound size={18} /> Change password</h2>
				<form class="flex flex-col gap-3" onsubmit={submitChange}>
					<label class="floating-label">
						<span>Current password</span>
						<input
							class="input w-full"
							type="password"
							bind:value={current}
							required
							autocomplete="current-password"
						/>
					</label>
					<label class="floating-label">
						<span>New password</span>
						<input
							class="input w-full"
							type="password"
							bind:value={next}
							required
							placeholder="at least 12 characters"
							autocomplete="new-password"
						/>
					</label>
					<button class="btn btn-sm w-fit" type="submit" disabled={busy}>Change password</button>
				</form>
			</div>
		</section>

		<section class="card card-border max-w-xl bg-base-200">
			<div class="card-body gap-4">
				<h2 class="card-title"><ShieldCheck size={18} /> Recovery codes</h2>
				<p class="text-sm text-base-content/70">
					If you forget your password and this instance has no email, a recovery code is how you get
					back in. Generate them, save them somewhere safe, and use one to set a new password.
				</p>
				<button class="btn btn-sm w-fit" onclick={showCodes} disabled={busy}>
					Generate new codes
				</button>
				{#if codes.length > 0}
					<ul class="grid grid-cols-2 gap-2 font-mono text-sm">
						{#each codes as code (code)}
							<li class="rounded-field bg-base-300 px-2 py-1">{code}</li>
						{/each}
					</ul>
					<p class="text-warning text-sm">
						These replace any earlier codes and are shown only once.
					</p>
				{/if}
			</div>
		</section>
	{:else}
		<section class="card card-border max-w-md bg-base-200">
			<div class="card-body gap-4">
				{#if recovering}
					<h2 class="text-lg font-semibold">Recover your account</h2>
					<p class="text-sm text-base-content/60">
						Enter one of your recovery codes and choose a new password.
					</p>
					<form class="flex flex-col gap-3" onsubmit={submitRecovery}>
						<label class="floating-label">
							<span>Email</span>
							<input class="input w-full" type="email" bind:value={recoveryEmail} required />
						</label>
						<label class="floating-label">
							<span>Recovery code</span>
							<input class="input w-full font-mono text-sm" bind:value={recoveryCode} required />
						</label>
						<label class="floating-label">
							<span>New password</span>
							<input
								class="input w-full"
								type="password"
								bind:value={recoveryNext}
								required
								autocomplete="new-password"
							/>
						</label>
						<button class="btn btn-primary" type="submit" disabled={busy}>Set a new password</button
						>
						<button class="btn btn-ghost btn-sm" type="button" onclick={() => (recovering = false)}>
							Back to sign in
						</button>
					</form>
				{:else}
					{#if registrationOpen}
						<div role="tablist" class="tabs tabs-box w-fit">
							<button
								role="tab"
								class="tab"
								class:tab-active={!creating}
								onclick={() => {
									creating = false;
									clear();
								}}>Sign in</button
							>
							<button
								role="tab"
								class="tab"
								class:tab-active={creating}
								onclick={() => {
									creating = true;
									clear();
								}}>Create account</button
							>
						</div>
					{:else}
						<h2 class="text-lg font-semibold">Sign in</h2>
						<p class="text-sm text-base-content/60">
							This instance is not accepting new accounts. Ask the operator for one.
						</p>
					{/if}
					<form class="flex flex-col gap-3" onsubmit={submit}>
						<label class="floating-label">
							<span>Email</span>
							<input
								class="input w-full"
								type="email"
								bind:value={email}
								placeholder="you@example.org"
								aria-label="Email"
								autocomplete="email"
							/>
						</label>
						<label class="floating-label">
							<span>Password</span>
							<input
								class="input w-full"
								type="password"
								bind:value={password}
								placeholder={creating ? 'at least 12 characters' : 'password'}
								aria-label="Password"
								autocomplete={creating ? 'new-password' : 'current-password'}
							/>
						</label>
						<button class="btn btn-primary" type="submit" disabled={busy}>
							{creating ? 'Create account' : 'Sign in'}
						</button>
					</form>
					<button
						class="btn btn-ghost btn-sm w-fit"
						type="button"
						onclick={() => (recovering = true)}
					>
						Forgot your password?
					</button>
					<p class="text-xs text-base-content/50">
						An account authenticates you to this server. It does not sign releases; your release key
						does, and it stays on your device.
					</p>
				{/if}
			</div>
		</section>
	{/if}
</div>
