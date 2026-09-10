<script lang="ts">
	import { goto } from '$app/navigation';
	import { Search } from '@lucide/svelte';
	import { untrack } from 'svelte';

	let {
		value = '',
		placeholder = 'Search mods, plugins, packs…',
		class: className = '',
	}: { value?: string; placeholder?: string; class?: string } = $props();

	let query = $state(untrack(() => value));

	function submit(event: SubmitEvent) {
		event.preventDefault();
		const trimmed = query.trim();
		goto(trimmed ? `/search?q=${encodeURIComponent(trimmed)}` : '/search');
	}
</script>

<form class={`join w-full ${className}`} onsubmit={submit} role="search">
	<label class="input join-item w-full">
		<Search size={16} class="opacity-50" />
		<input bind:value={query} {placeholder} aria-label="Search mods" autocomplete="off" />
	</label>
	<button class="btn join-item" type="submit">Search</button>
</form>
