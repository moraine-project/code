<script lang="ts">
	import { goto } from '$app/navigation';
	import { Menu, Moon, Sun } from '@lucide/svelte';
	import { onMount } from 'svelte';
	import '../app.css';
	import favicon from '$lib/assets/favicon.svg';
	import { emailVerificationMode } from '$lib/api/registry';
	import { logout, resendVerification } from '$lib/api/session';
	import Avatar from '$lib/components/Avatar.svelte';
	import Logo from '$lib/components/Logo.svelte';
	import SearchBar from '$lib/components/SearchBar.svelte';
	import { session } from '$lib/session.svelte';

	let { children } = $props();

	let verifyEnabled = $state(false);
	let verifyNotice = $state<string | null>(null);

	onMount(() => {
		session.refresh();
		void emailVerificationMode()
			.then((enabled) => (verifyEnabled = enabled))
			.catch(() => (verifyEnabled = false));
	});

	async function resendVerificationEmail() {
		verifyNotice = null;
		try {
			await resendVerification();
			verifyNotice = 'Sent.';
		} catch (cause) {
			verifyNotice = cause instanceof Error ? cause.message : 'could not send';
		}
	}

	let userMenu = $state<HTMLDetailsElement | null>(null);
	let mobileMenu = $state<HTMLDetailsElement | null>(null);

	function closeMenus() {
		if (userMenu) {
			userMenu.open = false;
		}
		if (mobileMenu) {
			mobileMenu.open = false;
		}
	}

	function dismiss(event: MouseEvent) {
		const target = event.target as Node;
		if (userMenu?.open && !userMenu.contains(target)) {
			userMenu.open = false;
		}
		if (mobileMenu?.open && !mobileMenu.contains(target)) {
			mobileMenu.open = false;
		}
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			closeMenus();
		}
	}

	async function signOut() {
		closeMenus();
		await logout().catch(() => undefined);
		session.set(null);
		await goto('/');
	}

	const nav = [
		['/games', 'Browse'],
		['/search', 'Mods'],
	] as const;

	const accountLinks = [
		['/account', 'Account'],
		['/submissions', 'Submissions'],
		['/orgs', 'Organizations'],
		['/notifications', 'Notifications'],
	] as const;
</script>

<svelte:head>
	<link rel="icon" href={favicon} type="image/svg+xml" />
	<meta name="theme-color" content="#0b1220" />
	<title>Moraine</title>
</svelte:head>

<svelte:window onclick={dismiss} onkeydown={onKeydown} />

<div class="flex min-h-screen flex-col bg-base-100 text-base-content">
	<a
		class="btn btn-sm sr-only focus:not-sr-only focus:absolute focus:left-2 focus:top-2 focus:z-50"
		href="#content"
	>
		Skip to content
	</a>

	<header class="sticky top-0 z-40 border-b border-base-300 bg-base-100/90 backdrop-blur">
		<div class="mx-auto flex w-full max-w-7xl items-center gap-2 px-3 py-2 sm:px-4">
			<Logo />
			<nav class="hidden items-center md:flex">
				{#each nav as [href, label] (href)}
					<a class="btn btn-ghost btn-sm" {href}>{label}</a>
				{/each}
			</nav>

			<div class="hidden flex-1 justify-center px-2 md:flex">
				<SearchBar class="max-w-lg" />
			</div>

			<div class="ml-auto flex items-center gap-1 md:ml-0">
				<a class="btn btn-primary btn-sm hidden sm:inline-flex" href="/publish">Publish</a>

				<label class="btn btn-ghost btn-square btn-sm" title="Switch theme">
					<span class="swap swap-rotate">
						<input
							type="checkbox"
							class="theme-controller"
							value="moraine-light"
							aria-label="Switch to the light theme"
						/>
						<Moon class="swap-off" size={18} />
						<Sun class="swap-on" size={18} />
					</span>
				</label>

				{#if session.user}
					<details class="dropdown dropdown-end" bind:this={userMenu}>
						<summary class="btn btn-ghost btn-circle" aria-label="Your account">
							<Avatar name={session.user.email} id={session.user.user_id} size={30} />
						</summary>
						<ul
							class="menu dropdown-content z-50 mt-2 w-56 rounded-box border border-base-300 bg-base-100 p-2 shadow-xl"
						>
							<li class="menu-title truncate">{session.user.email}</li>
							{#if session.user.role === 'operator'}
								<li><a href="/dashboard" onclick={closeMenus}>Dashboard</a></li>
							{/if}
							{#each accountLinks as [href, label] (href)}
								<li><a {href} onclick={closeMenus}>{label}</a></li>
							{/each}
							<li><a href="/publish" onclick={closeMenus}>Publish a mod</a></li>
							<li><button type="button" onclick={signOut}>Sign out</button></li>
						</ul>
					</details>
				{:else}
					<a class="btn btn-ghost btn-sm" href="/account">Sign in</a>
				{/if}

				<details class="dropdown dropdown-end md:hidden" bind:this={mobileMenu}>
					<summary class="btn btn-ghost btn-square btn-sm" aria-label="Menu">
						<Menu size={20} />
					</summary>
					<ul
						class="menu dropdown-content z-50 mt-2 w-60 rounded-box border border-base-300 bg-base-100 p-2 shadow-xl"
					>
						{#each nav as [href, label] (href)}
							<li><a {href} onclick={closeMenus}>{label}</a></li>
						{/each}
						<li><a href="/publish" onclick={closeMenus}>Publish a mod</a></li>
						{#if session.user?.role === 'operator'}
							<li><a href="/dashboard" onclick={closeMenus}>Dashboard</a></li>
						{/if}
						<li class="menu-title">Your account</li>
						{#each accountLinks as [href, label] (href)}
							<li><a {href} onclick={closeMenus}>{label}</a></li>
						{/each}
					</ul>
				</details>
			</div>
		</div>

		<div class="px-3 pb-3 md:hidden">
			<SearchBar />
		</div>
	</header>

	{#if verifyEnabled && session.user?.verified === false}
		<div class="border-b border-warning/40 bg-warning/10">
			<div class="mx-auto flex w-full max-w-7xl flex-wrap items-center gap-3 px-4 py-2 text-sm">
				<span>Verify your email to publish.</span>
				<button class="btn btn-xs" onclick={resendVerificationEmail}>Resend</button>
				{#if verifyNotice}
					<span class="text-base-content/60">{verifyNotice}</span>
				{/if}
			</div>
		</div>
	{/if}

	<main id="content" class="mx-auto w-full max-w-7xl flex-1 px-3 py-6 sm:px-4 lg:px-6">
		{@render children()}
	</main>

	<footer class="mt-8 border-t border-base-300 bg-base-200">
		<div class="mx-auto grid w-full max-w-7xl gap-8 px-4 py-10 sm:grid-cols-2 lg:grid-cols-4">
			<div class="flex flex-col gap-3">
				<Logo />
				<p class="max-w-xs text-sm text-base-content/70">
					Publish and find mods, plugins, and packs. Every project stays at a home its publisher
					controls, and anyone can run an instance.
				</p>
			</div>
			<nav class="flex flex-col gap-2 text-sm">
				<p class="font-medium text-base-content/60">Discover</p>
				<a class="link link-hover w-fit" href="/games">Browse games</a>
				<a class="link link-hover w-fit" href="/search">Search mods</a>
				<a class="link link-hover w-fit" href="/projects">Projects you follow</a>
			</nav>
			<nav class="flex flex-col gap-2 text-sm">
				<p class="font-medium text-base-content/60">For authors</p>
				<a class="link link-hover w-fit" href="/publish">Publish a mod</a>
				<a class="link link-hover w-fit" href="/submissions">Your submissions</a>
				<a class="link link-hover w-fit" href="/orgs">Organizations</a>
			</nav>
			<nav class="flex flex-col gap-2 text-sm">
				<p class="font-medium text-base-content/60">About</p>
				<a class="link link-hover w-fit" href="/about">How it works</a>
				<a class="link link-hover w-fit" href="/security">What checks prove</a>
				<a class="link link-hover w-fit" href="/instance">Instance information</a>
				<a class="link link-hover w-fit" href="/dashboard">Dashboard</a>
			</nav>
		</div>
		<div class="border-t border-base-300">
			<p class="mx-auto w-full max-w-7xl px-4 py-4 text-xs text-base-content/60">
				Moraine is free software, released under the AGPL-3.0-or-later.
			</p>
		</div>
	</footer>
</div>
