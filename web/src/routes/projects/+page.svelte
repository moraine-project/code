<script lang="ts">
	import { shortDigest } from '$lib/api/registry';
	import Digest from '$lib/components/Digest.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<svelte:head>
	<title>Projects · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<h1 class="text-2xl font-bold">Projects</h1>
	<p class="text-base-content/80 max-w-2xl">
		The projects you follow on this instance. Following a project makes it appear here, records
		notifications for its releases, and pulls its feed so a local copy exists. A follow is local and
		never changes anything the publisher signed.
	</p>

	{#if data.error}
		<div role="alert" class="alert alert-error"><span>{data.error}</span></div>
	{:else if data.projects.length === 0}
		<p class="text-base-content/60 text-sm">
			You are not following anything yet. Follow a project from its page.
		</p>
	{:else}
		<ul class="flex flex-col gap-3">
			{#each data.projects as project (project.id)}
				<li class="card card-border">
					<div class="card-body">
						<a
							class="card-title link link-hover"
							href={`/p/${encodeURIComponent(project.id)}${data.base ? `?home=${encodeURIComponent(data.base)}` : ''}`}
						>
							{project.name ?? shortDigest(project.id, 24)}
						</a>
						{#if project.summary}
							<p class="text-base-content/80 text-sm">{project.summary}</p>
						{/if}
						<Digest value={project.id} label="the project id" length={12} />
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</div>
