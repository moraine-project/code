<script lang="ts">
	import { onMount } from 'svelte';
	import { login, logout, register } from '$lib/api/session';
	import Avatar from '$lib/components/Avatar.svelte';
	import PageHeader from '$lib/components/PageHeader.svelte';
	import { session } from '$lib/session.svelte';

	let user = $state<typeof session.user>(null);
	let email = $state('');
	let password = $state('');
	let creating = $state(false);
	let busy = $state(false);
	let error = $state<string | null>(null);
	let notice = $state<string | null>(null);

	onMount(refresh);

	async function refresh() {
		user = await session.refresh();
	}

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		error = null;
		notice = null;
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

	async function signOut() {
		error = null;
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
	{:else}
		<section class="card card-border max-w-md bg-base-200">
			<div class="card-body gap-4">
				<div role="tablist" class="tabs tabs-box w-fit">
					<button
						role="tab"
						class="tab"
						class:tab-active={!creating}
						onclick={() => {
							creating = false;
							notice = null;
							error = null;
						}}>Sign in</button
					>
					<button
						role="tab"
						class="tab"
						class:tab-active={creating}
						onclick={() => {
							creating = true;
							notice = null;
							error = null;
						}}>Create account</button
					>
				</div>
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
				<p class="text-xs text-base-content/50">
					An account authenticates you to this server. It does not sign releases; your release key
					does, and it stays on your device.
				</p>
			</div>
		</section>
	{/if}
</div>
