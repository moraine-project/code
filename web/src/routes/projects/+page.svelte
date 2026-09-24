<script lang="ts">
	import Avatar from '$lib/components/Avatar.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import PageHeader from '$lib/components/PageHeader.svelte';
	import { shortDigest } from '$lib/api/digests';
	import { pageTitle } from '$lib/title.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<svelte:head>
	<title>{pageTitle('Following')}</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<PageHeader
		title="Following"
		subtitle="Projects you follow on this instance. Following keeps a local copy of the feed and notifies you about new releases."
	/>

	{#if data.error}
		<div role="alert" class="alert alert-error"><span>{data.error}</span></div>
	{:else if data.projects.length === 0}
		<EmptyState
			title="You are not following anything yet"
			message="Open a project and follow it to see its releases here."
		>
			<a class="btn btn-primary" href="/search">Find a mod</a>
		</EmptyState>
	{:else}
		<div class="grid gap-3 lg:grid-cols-2">
			{#each data.projects as project (project.id)}
				<a
					class="card card-border bg-base-200 transition hover:border-primary"
					href={`/p/${encodeURIComponent(project.id)}`}
				>
					<div class="card-body flex-row items-center gap-4">
						<Avatar name={project.name ?? 'Project'} id={project.id} size={44} />
						<div class="min-w-0">
							<p class="truncate font-semibold">{project.name ?? shortDigest(project.id, 24)}</p>
							{#if project.summary}
								<p class="line-clamp-1 text-sm text-base-content/70">{project.summary}</p>
							{/if}
						</div>
					</div>
				</a>
			{/each}
		</div>
	{/if}
</div>
