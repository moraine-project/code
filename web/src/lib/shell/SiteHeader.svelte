<script lang="ts">
	import { goto } from '$app/navigation';
	import { Menu, Moon, Sun } from '@lucide/svelte';
	import { logout, resendVerification } from '$lib/api/session';
	import Avatar from '$lib/components/Avatar.svelte';
	import Logo from '$lib/components/Logo.svelte';
	import SearchBar from '$lib/components/SearchBar.svelte';
	import { home } from '$lib/home.svelte';
	import { instance } from '$lib/instance.svelte';
	import { session } from '$lib/session.svelte';
	import { theme } from '$lib/theme.svelte';

	let verifyNotice = $state<string | null>(null);
	let userMenu = $state<HTMLDetailsElement | null>(null);
	let mobileMenu = $state<HTMLDetailsElement | null>(null);

	export function closeMenus() {
		if (userMenu) userMenu.open = false;
		if (mobileMenu) mobileMenu.open = false;
	}

	async function resendVerificationEmail() {
		verifyNotice = null;
		try {
			await resendVerification();
			verifyNotice = 'Sent.';
		} catch (cause) {
			verifyNotice = cause instanceof Error ? cause.message : 'could not send';
		}
	}

	async function signOut() {
		closeMenus();
		await logout().catch(() => undefined);
		session.set(null);
		await goto('/');
	}
	const primaryNav = $derived([
		[home.url('/games'), 'Browse'],
		[home.url('/search'), 'Mods'],
		...instance.nav.map(
			(link) => [link.href.startsWith('/') ? home.url(link.href) : link.href, link.label] as const,
		),
	]);

	const accountLinks = [
		['/account', 'Account'],
		['/submissions', 'Submissions'],
		['/orgs', 'Organizations'],
		['/notifications', 'Notifications'],
	] as const;
</script>

<header class="sticky top-0 z-40 border-b border-base-300 bg-base-100/90 backdrop-blur">
	<div class="mx-auto flex w-full max-w-7xl items-center gap-2 px-3 py-2 sm:px-4">
		<Logo />
		<nav class="hidden items-center md:flex">
			{#each primaryNav as [href, label] (href)}
				<a class="btn btn-ghost btn-sm" {href}>{label}</a>
			{/each}
		</nav>

		<div class="hidden flex-1 justify-center px-2 md:flex">
			<SearchBar class="max-w-lg" />
		</div>

		<div class="ml-auto flex items-center gap-1 md:ml-0">
			<a class="btn btn-primary btn-sm hidden sm:inline-flex" href="/publish">Publish</a>

			<button
				class="btn btn-ghost btn-square btn-sm"
				type="button"
				title="Switch theme"
				aria-label={theme.isLight ? 'Switch to the dark theme' : 'Switch to the light theme'}
				onclick={() => theme.toggle()}
			>
				{#if theme.isLight}
					<Sun size={18} />
				{:else}
					<Moon size={18} />
				{/if}
			</button>

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
					{#each primaryNav as [href, label] (href)}
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

{#if instance.emailVerification && session.user?.verified === false}
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
