<script lang="ts">
	import '../app.css';
	import favicon from '$lib/assets/favicon.svg';

	let { children } = $props();
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<title>Moraine</title>
</svelte:head>

<div class="min-h-screen bg-base-100">
	<div class="navbar bg-base-200">
		<div class="navbar-start">
			<a class="btn btn-ghost text-xl" href="/">Moraine</a>
		</div>
		<div class="navbar-end gap-2">
			<a class="btn btn-ghost" href="/search">Search</a>
			<a class="btn btn-ghost" href="/account">Account</a>
			<span class="badge badge-ghost">federated registry</span>
		</div>
	</div>

	<main class="mx-auto w-full max-w-5xl p-4 sm:p-8">
		{@render children()}
	</main>
</div>
