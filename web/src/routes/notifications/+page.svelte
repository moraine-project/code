<script lang="ts">
	import { onMount } from 'svelte';
	import { shortDigest } from '$lib/api/registry';
	import {
		follows,
		notifications,
		readAll,
		readNotification,
		type Notification,
	} from '$lib/api/notifications';
	import Digest from '$lib/components/Digest.svelte';

	let items = $state<Notification[]>([]);
	let followed = $state<string[]>([]);
	let error = $state<string | null>(null);
	let loading = $state(true);

	onMount(load);

	async function load() {
		loading = true;
		try {
			[items, followed] = await Promise.all([notifications(false), follows()]);
			error = null;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not load notifications';
		} finally {
			loading = false;
		}
	}

	async function markRead(notification: Notification) {
		try {
			await readNotification(notification.id);
			items = items.map((item) => (item.id === notification.id ? { ...item, read: true } : item));
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not mark it read';
		}
	}

	async function markAll() {
		try {
			await readAll();
			items = items.map((item) => ({ ...item, read: true }));
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not mark them read';
		}
	}

	function formatTime(seconds: number): string {
		return new Date(seconds * 1000).toLocaleString();
	}
</script>

<svelte:head>
	<title>Notifications · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<div class="flex flex-wrap items-center justify-between gap-2">
		<h1 class="text-2xl font-bold">Notifications</h1>
		{#if items.some((item) => !item.read)}
			<button class="btn btn-sm" onclick={markAll}>Mark all read</button>
		{/if}
	</div>

	{#if error}
		<div role="alert" class="alert alert-error"><span>{error}</span></div>
	{:else if loading}
		<span class="loading loading-spinner loading-md"></span>
	{:else if items.length === 0}
		<p class="text-base-content/60 text-sm">
			No notifications. Follow a project and its accepted feed events show up here.
		</p>
	{:else}
		<ul class="flex flex-col gap-2">
			{#each items as item (item.id)}
				<li class="card card-border">
					<div class="card-body flex-row items-center justify-between gap-2 py-3">
						<div class="flex flex-col gap-1">
							<Digest value={item.project_id} label="the project id" length={12} />
							<span class="text-sm">
								<strong>{item.event_kind}</strong>
								{#if item.feed_seq !== null && item.feed_seq !== undefined}
									at seq {item.feed_seq}
								{/if}
								— {formatTime(item.created_at)}
							</span>
						</div>
						{#if item.read}
							<span class="badge badge-ghost">read</span>
						{:else}
							<button class="btn btn-sm btn-outline" onclick={() => markRead(item)}
								>Mark read</button
							>
						{/if}
					</div>
				</li>
			{/each}
		</ul>
	{/if}

	{#if followed.length > 0}
		<section class="flex flex-col gap-1 text-sm">
			<h2 class="text-base-content/60">Following</h2>
			<ul class="flex flex-wrap gap-2">
				{#each followed as project (project)}
					<li class="badge badge-outline font-mono">{shortDigest(project)}</li>
				{/each}
			</ul>
		</section>
	{/if}
</div>
