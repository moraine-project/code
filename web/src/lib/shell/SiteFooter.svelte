<script lang="ts">
	import Logo from '$lib/components/Logo.svelte';
	import { home } from '$lib/home.svelte';
	import { instance } from '$lib/instance.svelte';

	const discover = [
		['/games', 'Browse games'],
		['/search', 'Search mods'],
		['/projects', 'Projects you follow'],
	] as const;

	const authors = [
		['/publish', 'Publish a mod'],
		['/submissions', 'Your submissions'],
		['/orgs', 'Organizations'],
	] as const;

	const about = [
		['/about', 'How it works'],
		['/security', 'What checks prove'],
		['/instance', 'Instance information'],
		['/dashboard', 'Dashboard'],
	] as const;

	const operatorLinks = $derived(
		instance.nav.map(
			(link) => [link.href.startsWith('/') ? home.url(link.href) : link.href, link.label] as const,
		),
	);
</script>

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
			{#each discover as [path, label] (path)}
				<a class="link link-hover w-fit" href={home.url(path)}>{label}</a>
			{/each}
		</nav>
		<nav class="flex flex-col gap-2 text-sm">
			<p class="font-medium text-base-content/60">For authors</p>
			{#each authors as [path, label] (path)}
				<a class="link link-hover w-fit" href={home.url(path)}>{label}</a>
			{/each}
		</nav>
		<nav class="flex flex-col gap-2 text-sm">
			<p class="font-medium text-base-content/60">About</p>
			{#each about as [path, label] (path)}
				<a class="link link-hover w-fit" href={home.url(path)}>{label}</a>
			{/each}
			{#each operatorLinks as [href, label] (href)}
				<a class="link link-hover w-fit" {href}>{label}</a>
			{/each}
		</nav>
	</div>
	<div class="border-t border-base-300">
		<p class="mx-auto w-full max-w-7xl px-4 py-4 text-xs text-base-content/60">
			{instance.name} is free software, released under the AGPL-3.0-or-later.
		</p>
	</div>
</footer>
