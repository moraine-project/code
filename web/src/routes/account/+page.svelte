<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { Database, KeyRound, ShieldCheck, Trash2 } from '@lucide/svelte';
	import {
		changePassword,
		deleteAccount,
		exportAccount,
		issueRecoveryCodes,
		login,
		logout,
		recover,
		register,
		resendVerification,
		verifyEmail,
		apiKeys,
		createApiKey,
		revokeApiKey,
		type ApiKey,
	} from '$lib/api/session';
	import { instance } from '$lib/instance.svelte';
	import { createWebhook, revokeWebhook, webhooks, type Webhook } from '$lib/api/webhooks';
	import Avatar from '$lib/components/Avatar.svelte';
	import PageHeader from '$lib/components/PageHeader.svelte';
	import { session } from '$lib/session.svelte';
	import { pageTitle } from '$lib/title.svelte';

	let user = $state<typeof session.user>(null);
	let email = $state('');
	let password = $state('');
	let creating = $state(false);
	let recovering = $state(false);
	let busy = $state(false);
	let error = $state<string | null>(null);
	let notice = $state<string | null>(null);
	let registrationOpen = $derived(instance.registration === 'open');

	let current = $state('');
	let next = $state('');
	let codes = $state<string[]>([]);
	let recoveryEmail = $state('');
	let recoveryCode = $state('');
	let recoveryNext = $state('');
	let webhookUrl = $state('');
	let webhookKinds = $state('release-published,profile-updated');
	let webhookItems = $state<Webhook[]>([]);
	let keyItems = $state<ApiKey[]>([]);
	let keyName = $state('');
	let keyScopes = $state('projects:write,artifacts:write');
	let newKey = $state<string | null>(null);

	onMount(() => {
		void refresh();
		const token = page.url.searchParams.get('verify');
		if (token) {
			void verifyEmail(token)
				.then(() => {
					notice = 'Email verified.';
					return refresh();
				})
				.catch((cause) => {
					error = cause instanceof Error ? cause.message : 'the verification failed';
				});
		}
	});

	async function exportData() {
		clear();
		busy = true;
		try {
			const data = await exportAccount();
			const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
			const url = URL.createObjectURL(blob);
			const anchor = document.createElement('a');
			anchor.href = url;
			anchor.download = 'moraine-account.json';
			anchor.click();
			URL.revokeObjectURL(url);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'the export failed';
		} finally {
			busy = false;
		}
	}

	async function resend() {
		clear();
		busy = true;
		try {
			await resendVerification();
			notice = 'Verification email sent.';
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not resend';
		} finally {
			busy = false;
		}
	}

	async function deleteSelf() {
		if (!confirm('Delete your account? This cannot be undone.')) {
			return;
		}
		clear();
		busy = true;
		try {
			await deleteAccount();
			session.set(null);
			user = null;
			notice = 'Your account was deleted.';
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not delete the account';
		} finally {
			busy = false;
		}
	}

	async function refresh() {
		user = await session.refresh();
		if (user) webhookItems = await webhooks().catch(() => []);
		if (user) keyItems = await apiKeys().catch(() => []);
	}

	async function addApiKey(event: SubmitEvent) {
		event.preventDefault();
		clear();
		busy = true;
		try {
			const created = await createApiKey(
				keyName,
				keyScopes
					.split(',')
					.map((scope) => scope.trim())
					.filter(Boolean),
				90,
			);
			newKey = created.key;
			keyName = '';
			keyItems = await apiKeys();
			notice = 'API key created. Copy it now; it will not be shown again.';
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not create the API key';
		} finally {
			busy = false;
		}
	}

	async function removeApiKey(item: ApiKey) {
		try {
			await revokeApiKey(item.id);
			keyItems = keyItems.filter((key) => key.id !== item.id);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not revoke the API key';
		}
	}

	async function addWebhook(event: SubmitEvent) {
		event.preventDefault();
		clear();
		busy = true;
		try {
			await createWebhook(
				webhookUrl,
				webhookKinds
					.split(',')
					.map((kind) => kind.trim())
					.filter(Boolean),
			);
			webhookUrl = '';
			webhookItems = await webhooks();
			notice = 'Webhook added.';
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not add the webhook';
		} finally {
			busy = false;
		}
	}

	async function removeWebhook(item: Webhook) {
		clear();
		try {
			await revokeWebhook(item.id);
			webhookItems = webhookItems.filter((webhook) => webhook.id !== item.id);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not revoke the webhook';
		}
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
	<title>{pageTitle('Account')}</title>
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
						<p class="text-sm text-base-content/60">
							Signed in by {user.via}
							{#if user.verified === false}
								· <span class="text-warning">email not verified</span>
							{/if}
						</p>
					</div>
				</div>
				{#if user.verified === false}
					<button class="btn btn-sm btn-outline w-fit" onclick={resend} disabled={busy}>
						Resend the verification email
					</button>
				{/if}
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
				<h2 class="card-title">API keys</h2>
				<p class="text-sm text-base-content/70">
					Use a scoped bearer key for CI or other automation. The secret is shown once.
				</p>
				<form class="flex flex-col gap-3" onsubmit={addApiKey}>
					<input
						class="input w-full"
						bind:value={keyName}
						required
						placeholder="release bot"
						aria-label="API key name"
					/>
					<input class="input w-full" bind:value={keyScopes} aria-label="API key scopes" />
					<button class="btn btn-sm w-fit" type="submit" disabled={busy}>Create API key</button>
				</form>
				{#if newKey}<pre
						class="overflow-x-auto rounded-box bg-base-300 p-3 text-xs"
						aria-label="New API key">{newKey}</pre>{/if}
				{#if keyItems.length > 0}
					<ul class="flex flex-col gap-2 text-sm">
						{#each keyItems as item (item.id)}
							<li
								class="flex flex-wrap items-center justify-between gap-2 border-b border-base-300 pb-2"
							>
								<span
									><strong>{item.name}</strong>
									<span class="font-mono text-xs">{item.prefix}…</span><br /><span
										class="text-xs text-base-content/60"
										>{item.scopes.join(', ') || 'no scopes'}</span
									></span
								>
								<button
									class="btn btn-ghost btn-xs"
									type="button"
									onclick={() => removeApiKey(item)}>Revoke</button
								>
							</li>
						{/each}
					</ul>
				{/if}
			</div>
		</section>

		<section class="card card-border max-w-xl bg-base-200">
			<div class="card-body gap-4">
				<h2 class="card-title">Release webhooks</h2>
				<p class="text-sm text-base-content/70">
					Send signed release events to your automation endpoint.
				</p>
				<form class="flex flex-col gap-3" onsubmit={addWebhook}>
					<input
						class="input w-full"
						type="url"
						bind:value={webhookUrl}
						required
						placeholder="https://example.org/moraine-hook"
						aria-label="Webhook URL"
					/>
					<input class="input w-full" bind:value={webhookKinds} aria-label="Webhook event kinds" />
					<button class="btn btn-sm w-fit" type="submit" disabled={busy}>Add webhook</button>
				</form>
				{#if webhookItems.length > 0}
					<ul class="flex flex-col gap-2 text-sm">
						{#each webhookItems as item (item.id)}
							<li
								class="flex flex-wrap items-center justify-between gap-2 border-b border-base-300 pb-2"
							>
								<span
									><span class="font-mono">{item.url}</span><br /><span
										class="text-xs text-base-content/60"
										>{item.event_kinds.join(', ') || 'all events'}</span
									></span
								>
								<button
									class="btn btn-ghost btn-xs"
									type="button"
									onclick={() => removeWebhook(item)}>Revoke</button
								>
							</li>
						{/each}
					</ul>
				{/if}
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
				<h2 class="card-title"><Database size={18} /> Your data</h2>
				<p class="text-sm text-base-content/70">
					Download everything this instance holds about your account, or delete the account. Signed
					projects and releases stay, because they are published under your key, not your account.
				</p>
				<div class="flex flex-wrap gap-2">
					<button class="btn btn-sm btn-outline" onclick={exportData} disabled={busy}>
						<Database size={15} />
						Export my data
					</button>
					<button class="btn btn-sm btn-outline" onclick={deleteSelf} disabled={busy}>
						<Trash2 size={15} />
						Delete my account
					</button>
				</div>
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
						<div class="tabs tabs-box w-fit" aria-label="Account access">
							<button
								type="button"
								class="tab"
								class:tab-active={!creating}
								aria-pressed={!creating}
								onclick={() => {
									creating = false;
									clear();
								}}>Sign in</button
							>
							<button
								type="button"
								class="tab"
								class:tab-active={creating}
								aria-pressed={creating}
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
