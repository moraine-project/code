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
			<a class="btn btn-ghost" href="/games">Browse</a>
			<a class="btn btn-ghost" href="/search">Search</a>
			<a class="btn btn-ghost" href="/publish">Publish</a>
			<a class="btn btn-ghost" href="/submissions">Submissions</a>
			<a class="btn btn-ghost" href="/notifications">Notifications</a>
			<a class="btn btn-ghost" href="/orgs">Organizations</a>
			<a class="btn btn-ghost" href="/account">Account</a>
			<a class="btn btn-ghost" href="/settings">Settings</a>
			<a class="btn btn-ghost" href="/about">About</a>
			<a class="btn btn-ghost" href="/security">Security</a>
			<span class="badge badge-ghost">federated registry</span>
		</div>
	</div>

	<main class="mx-auto w-full max-w-5xl p-4 sm:p-8">
		{@render children()}
	</main>
</div>
