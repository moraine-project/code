<script lang="ts">
	import { page } from '$app/state';
	import { Home, SearchX } from '@lucide/svelte';
	import { pageTitle } from '$lib/title.svelte';

	const notFound = $derived(page.status === 404);
</script>

<svelte:head>
	<title>{pageTitle(String(page.status))}</title>
</svelte:head>

<div class="flex flex-col items-center gap-4 py-24 text-center">
	<span class="flex h-16 w-16 items-center justify-center rounded-box bg-base-200">
		<SearchX size={28} class="opacity-60" />
	</span>
	<p class="text-5xl font-bold tracking-tight text-base-content/20">{page.status}</p>
	<h1 class="text-xl font-semibold">
		{notFound ? 'That page is not here' : (page.error?.message ?? 'Something went wrong')}
	</h1>
	<p class="max-w-md text-base-content/70">
		{notFound
			? 'The link may be wrong, or the project may have moved to a different home.'
			: 'An error stopped this page from loading. Try again, or head back home.'}
	</p>
	<div class="flex flex-wrap justify-center gap-2 pt-1">
		<a class="btn btn-primary" href="/">
			<Home size={16} />
			Back home
		</a>
		<a class="btn btn-outline" href="/search">Search mods</a>
	</div>
</div>
