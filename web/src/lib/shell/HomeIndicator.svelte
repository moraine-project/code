<script lang="ts">
	import { home } from '$lib/home.svelte';
	import { session } from '$lib/session.svelte';
</script>

{#if home.isForeign}
	<div class="border-b border-info/40 bg-info/10" role="status">
		<div class="mx-auto flex w-full max-w-7xl flex-wrap items-center gap-2 px-4 py-2 text-sm">
			<span class="badge badge-info badge-sm">Read only</span>
			<span>
				Showing {home.base}. Your session belongs to {home.configured || 'this site'}, so account
				actions stay there.
			</span>
			<a
				class="btn btn-xs"
				href={home.configured ? `/?home=${encodeURIComponent(home.configured)}` : '/'}
			>
				Back to {home.configured || 'your home'}
			</a>
			{#if !session.user}
				<a class="btn btn-xs btn-outline" href="/account">Sign in</a>
			{/if}
		</div>
	</div>
{/if}
