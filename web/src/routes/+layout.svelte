<script lang="ts">
	import '../app.css';
	import favicon from '$lib/assets/favicon.svg';

	let { children } = $props();

	let menu = $state<HTMLDetailsElement | null>(null);

	function dismiss(event: MouseEvent) {
		if (menu?.open && !menu.contains(event.target as Node)) {
			menu.open = false;
		}
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape' && menu?.open) {
			menu.open = false;
			menu.focus();
		}
	}

	function close() {
		if (menu) {
			menu.open = false;
		}
	}

	const links = [
		['/games', 'Browse'],
		['/search', 'Search'],
		['/publish', 'Publish'],
		['/submissions', 'Submissions'],
		['/notifications', 'Notifications'],
		['/orgs', 'Organizations'],
		['/account', 'Account'],
		['/settings', 'Settings'],
		['/about', 'About'],
		['/security', 'Security']
	] as const;
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<title>Moraine</title>
</svelte:head>

<svelte:window onclick={dismiss} onkeydown={onKeydown} />

<div class="min-h-screen bg-base-100">
	<a
		class="btn btn-sm sr-only focus:not-sr-only focus:absolute focus:left-2 focus:top-2 focus:z-50"
		href="#content"
	>
		Skip to content
	</a>

	<div class="navbar bg-base-200">
		<div class="navbar-start">
			<a class="btn btn-ghost text-xl" href="/">Moraine</a>
		</div>
		<div class="navbar-end hidden gap-2 lg:flex">
			{#each links as [href, label] (href)}
				<a class="btn btn-ghost" {href}>{label}</a>
			{/each}
			<span class="badge badge-ghost">federated registry</span>
		</div>
		<div class="navbar-end lg:hidden">
			<details class="dropdown dropdown-end" bind:this={menu}>
				<summary class="btn btn-ghost" aria-label="Open navigation">Menu</summary>
				<ul class="menu menu-sm dropdown-content bg-base-100 rounded-box z-50 mt-3 w-52 p-2 shadow">
					{#each links as [href, label] (href)}
						<li><a {href} onclick={close}>{label}</a></li>
					{/each}
				</ul>
			</details>
		</div>
	</div>

	<main id="content" class="mx-auto w-full max-w-5xl p-4 sm:p-8">
		{@render children()}
	</main>
</div>
