<script lang="ts">
	import { onMount } from 'svelte';
	import { Bell, CheckCheck, Circle, GitBranch, Package, Tag, UserPen } from '@lucide/svelte';
	import { shortDigest } from '$lib/api/registry';
	import {
		follows,
		notifications,
		readAll,
		readNotification,
		type Notification,
	} from '$lib/api/notifications';
	import Avatar from '$lib/components/Avatar.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import PageHeader from '$lib/components/PageHeader.svelte';

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

	const iconFor = {
		'release-published': Package,
		'profile-updated': UserPen,
		'key-changed': GitBranch,
		advisory: Tag,
	} as const;

	const unread = $derived(items.filter((item) => !item.read).length);
</script>

<svelte:head>
	<title>Notifications · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<PageHeader title="Notifications" subtitle="Events from the projects you follow.">
		{#if unread > 0}
			<button class="btn btn-sm" onclick={markAll}>
				<CheckCheck size={16} />
				Mark all read
			</button>
		{/if}
	</PageHeader>

	{#if error}
		<div role="alert" class="alert alert-error"><span>{error}</span></div>
	{:else if loading}
		<div class="flex flex-col gap-2">
			<div class="skeleton h-16 w-full"></div>
			<div class="skeleton h-16 w-full"></div>
		</div>
	{:else if items.length === 0}
		<EmptyState
			title="No notifications"
			message="Follow a project and its accepted feed events show up here."
		>
			<a class="btn btn-primary" href="/search">Find a mod</a>
		</EmptyState>
	{:else}
		<ul class="flex flex-col gap-2">
			{#each items as item (item.id)}
				{@const Icon = iconFor[item.event_kind as keyof typeof iconFor] ?? Bell}
				<li
					class="flex items-center gap-4 rounded-box border border-base-300 bg-base-200 p-3"
					class:opacity-60={item.read}
				>
					<span class="flex h-9 w-9 shrink-0 items-center justify-center rounded-box bg-base-300">
						<Icon size={18} />
					</span>
					<div class="min-w-0 flex-1">
						<p class="text-sm font-medium">{item.event_kind}</p>
						<p class="text-xs text-base-content/60">
							{shortDigest(item.project_id, 20)}
							{#if item.feed_seq !== null && item.feed_seq !== undefined}
								· #{item.feed_seq}
							{/if}
							· {formatTime(item.created_at)}
						</p>
					</div>
					{#if item.read}
						<span class="badge badge-ghost badge-sm">read</span>
					{:else}
						<button class="btn btn-sm btn-ghost" onclick={() => markRead(item)}>
							<Circle size={14} class="fill-current" />
							Mark read
						</button>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}

	{#if followed.length > 0}
		<section class="flex flex-col gap-3">
			<h2 class="text-sm font-semibold text-base-content/60">Following</h2>
			<div class="flex flex-wrap gap-2">
				{#each followed as project (project)}
					<a class="badge badge-outline gap-2 py-3" href={`/p/${encodeURIComponent(project)}`}>
						<Avatar name={shortDigest(project, 8)} id={project} size={18} />
						{shortDigest(project, 12)}
					</a>
				{/each}
			</div>
		</section>
	{/if}
</div>
