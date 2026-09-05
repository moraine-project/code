<script lang="ts">
	import { onMount } from 'svelte';
	import '../app.css';
	import favicon from '$lib/assets/favicon.svg';
	import { session } from '$lib/session.svelte';

	let { children } = $props();

	onMount(() => {
		session.refresh();
	});

	let more = $state<HTMLDetailsElement | null>(null);

	function dismiss(event: MouseEvent) {
		if (more?.open && !more.contains(event.target as Node)) {
			more.open = false;
		}
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape' && more?.open) {
			more.open = false;
			more.focus();
		}
	}

	function close() {
		if (more) {
			more.open = false;
		}
	}

	const primary = [
		['/games', 'Browse'],
		['/search', 'Search'],
		['/projects', 'Projects'],
		['/orgs', 'Organizations'],
	] as const;

	const secondary = [
		['/submissions', 'Submissions'],
		['/notifications', 'Notifications'],
		['/account', 'Account'],
		['/settings', 'Settings'],
		['/about', 'About federation'],
		['/security', 'Security'],
	] as const;
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
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

	<header class="sticky top-0 z-40 border-b border-base-300 bg-base-200">
		<div class="navbar mx-auto w-full max-w-6xl gap-2 px-2">
			<div class="navbar-start gap-2">
				<a class="btn btn-ghost px-2 text-lg font-semibold tracking-tight" href="/">
					Moraine
					<span class="hidden text-xs font-normal text-base-content/60 md:inline">
						publish and find game mods
					</span>
				</a>
			</div>

			<div class="navbar-center hidden lg:flex">
				<ul class="menu menu-horizontal gap-1 px-1">
					{#each primary as [href, label] (href)}
						<li><a {href}>{label}</a></li>
					{/each}
				</ul>
			</div>

			<div class="navbar-end gap-1">
				<a class="btn btn-primary btn-sm hidden sm:inline-flex" href="/publish">Publish</a>

				<label class="btn btn-ghost btn-square btn-sm" title="Switch between dark and light">
					<span class="swap swap-rotate">
						<input
							type="checkbox"
							class="theme-controller"
							value="moraine-light"
							aria-label="Switch to the light theme"
						/>
						<svg
							class="swap-off h-5 w-5"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							stroke-width="1.8"
							stroke-linecap="round"
							stroke-linejoin="round"
							aria-hidden="true"
						>
							<path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
						</svg>
						<svg
							class="swap-on h-5 w-5"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							stroke-width="1.8"
							stroke-linecap="round"
							stroke-linejoin="round"
							aria-hidden="true"
						>
							<circle cx="12" cy="12" r="4" />
							<path
								d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41"
							/>
						</svg>
					</span>
				</label>

				<div class="hidden items-center gap-2 sm:flex">
					{#if session.user}
						<span class="badge badge-ghost">{session.user.email}</span>
					{:else}
						<a class="btn btn-ghost btn-sm" href="/account">Sign in</a>
					{/if}
				</div>

				<details class="dropdown dropdown-end" bind:this={more}>
					<summary class="btn btn-ghost btn-sm" aria-label="More navigation">
						<svg
							class="h-5 w-5 lg:hidden"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							stroke-width="1.8"
							stroke-linecap="round"
							aria-hidden="true"
						>
							<path d="M4 6h16M4 12h16M4 18h16" />
						</svg>
						<span class="hidden lg:inline">More</span>
					</summary>
					<ul
						class="menu dropdown-content z-50 mt-2 w-56 rounded-box border border-base-300 bg-base-100 p-2"
					>
						<li class="menu-title lg:hidden">Browse</li>
						{#each primary as [href, label] (href)}
							<li class="lg:hidden"><a {href} onclick={close}>{label}</a></li>
						{/each}
						<li class="lg:hidden"><a href="/publish" onclick={close}>Publish</a></li>
						<li class="menu-title">Your account</li>
						{#each secondary as [href, label] (href)}
							<li><a {href} onclick={close}>{label}</a></li>
						{/each}
						<li class="sm:hidden">
							<a href="/account" onclick={close}>{session.user ? session.user.email : 'Sign in'}</a>
						</li>
					</ul>
				</details>
			</div>
		</div>
	</header>

	<main id="content" class="mx-auto w-full max-w-6xl flex-1 p-4 sm:p-6 lg:p-8">
		{@render children()}
	</main>

	<footer class="border-t border-base-300 bg-base-200">
		<div
			class="mx-auto flex w-full max-w-6xl flex-col gap-2 p-4 text-sm text-base-content/70 sm:flex-row sm:items-center sm:justify-between"
		>
			<p>Moraine is free software. Anyone can run an instance.</p>
			<nav class="flex gap-4">
				<a class="link link-hover" href="/about">How federation works</a>
				<a class="link link-hover" href="/security">What checks prove</a>
			</nav>
		</div>
	</footer>
</div>
