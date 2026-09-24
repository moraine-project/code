<script lang="ts">
	import { onMount } from 'svelte';
	import '../app.css';
	import bundledFavicon from '$lib/assets/favicon.svg';
	import { home } from '$lib/home.svelte';
	import { instance } from '$lib/instance.svelte';
	import HomeIndicator from '$lib/shell/HomeIndicator.svelte';
	import SiteFooter from '$lib/shell/SiteFooter.svelte';
	import SiteHeader from '$lib/shell/SiteHeader.svelte';
	import { session } from '$lib/session.svelte';
	import { theme } from '$lib/theme.svelte';
	import { pageTitle } from '$lib/title.svelte';

	let { children } = $props();

	onMount(() => {
		theme.restore();
		session.refresh();
	});

	$effect(() => {
		void instance.load(home.base, fetch);
	});

	$effect(() => {
		theme.applyTokens(document.documentElement, instance.theme);
		return () => theme.applyTokens(document.documentElement, {});
	});

	const icon = $derived(instance.logoUrl ?? bundledFavicon);
	const themeColor = $derived(instance.theme['base-100'] ?? '#0b1220');

	function dismiss(event: MouseEvent) {
		const target = event.target as Node;
		for (const menu of document.querySelectorAll<HTMLDetailsElement>('details.dropdown[open]')) {
			if (!menu.contains(target)) {
				menu.open = false;
			}
		}
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			for (const menu of document.querySelectorAll<HTMLDetailsElement>('details.dropdown[open]')) {
				menu.open = false;
			}
		}
	}
</script>

<svelte:head>
	<link rel="icon" href={icon} />
	<meta name="theme-color" content={themeColor} />
	<title>{pageTitle()}</title>
</svelte:head>

<svelte:window onclick={dismiss} onkeydown={onKeydown} />

<div class="flex min-h-screen flex-col bg-base-100 text-base-content">
	<a
		class="btn btn-sm sr-only focus:not-sr-only focus:absolute focus:left-2 focus:top-2 focus:z-50"
		href="#content"
	>
		Skip to content
	</a>

	<HomeIndicator />
	<SiteHeader />

	<main id="content" class="mx-auto w-full max-w-7xl flex-1 px-3 py-6 sm:px-4 lg:px-6">
		{@render children()}
	</main>

	<SiteFooter />
</div>
