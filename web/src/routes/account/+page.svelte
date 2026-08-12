<script lang="ts">
	import { onMount } from 'svelte';
	import { login, logout, register } from '$lib/api/session';
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
	<h1 class="text-2xl font-bold">Account</h1>

	{#if error}
		<div role="alert" class="alert alert-error"><span>{error}</span></div>
	{/if}
	{#if notice}
		<div role="alert" class="alert alert-info"><span>{notice}</span></div>
	{/if}

	{#if user}
		<section class="card card-border">
			<div class="card-body">
				<h2 class="card-title">{user.email}</h2>
				<p class="text-base-content/60 text-sm">Signed in by {user.via}.</p>
				<div class="card-actions">
					<a class="btn" href="/review">Review queue</a>
					<button class="btn btn-outline" onclick={signOut} disabled={busy}>Sign out</button>
				</div>
			</div>
		</section>
	{:else}
		<section class="card card-border max-w-md">
			<div class="card-body">
				<h2 class="card-title">{creating ? 'Create an account' : 'Sign in'}</h2>
				<p class="text-base-content/80 text-sm">
					Accounts authenticate you to this server. They do not sign releases; a release key does
					that, and only the key holder can publish.
				</p>
				<form class="flex flex-col gap-3" onsubmit={submit}>
					<input
						class="input w-full"
						type="email"
						bind:value={email}
						placeholder="you@example.org"
						aria-label="Email"
						autocomplete="email"
					/>
					<input
						class="input w-full"
						type="password"
						bind:value={password}
						placeholder={creating ? 'at least 12 characters' : 'password'}
						aria-label="Password"
						autocomplete={creating ? 'new-password' : 'current-password'}
					/>
					<div class="card-actions justify-between">
						<button class="btn" type="submit" disabled={busy}>
							{creating ? 'Create account' : 'Sign in'}
						</button>
						<button
							class="btn btn-ghost"
							type="button"
							onclick={() => {
								creating = !creating;
								notice = null;
								error = null;
							}}
						>
							{creating ? 'Have an account?' : 'Create one'}
						</button>
					</div>
				</form>
			</div>
		</section>
	{/if}
</div>
