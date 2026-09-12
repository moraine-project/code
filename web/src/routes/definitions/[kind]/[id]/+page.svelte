<script lang="ts">
	import { ChevronLeft, FileCode } from '@lucide/svelte';
	import Digest from '$lib/components/Digest.svelte';
	import PageHeader from '$lib/components/PageHeader.svelte';
	import SourceBadge from '$lib/components/SourceBadge.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const payload = $derived(
		data.definition?.payload && typeof data.definition.payload === 'object'
			? (data.definition.payload as Record<string, unknown>)
			: null,
	);

	function value(input: unknown): string {
		if (input === null || input === undefined) return '—';
		if (typeof input === 'string') return input;
		if (typeof input === 'number' || typeof input === 'boolean') return String(input);
		return JSON.stringify(input);
	}
</script>

<svelte:head>
	<title>{data.definition?.display_name ?? data.id} · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<a class="link link-hover inline-flex w-fit items-center gap-1 text-sm" href="/games">
		<ChevronLeft size={16} />
		Back to browse
	</a>

	{#if data.error}
		<div role="alert" class="alert alert-error"><span>{data.error}</span></div>
	{:else if data.definition}
		<PageHeader
			title={data.definition.display_name ?? 'Unnamed definition'}
			subtitle="The current signed description installers use: which versions exist, how they order, and how mods are set up."
		>
			<span class="badge badge-outline">{data.definition.kind}</span>
			<SourceBadge sourceHome={data.definition.source_home} />
		</PageHeader>

		<section class="card card-border bg-base-200">
			<div class="card-body gap-3">
				<div class="flex flex-wrap items-center gap-3 text-sm text-base-content/60">
					<span class="inline-flex items-center gap-1">
						identity <Digest value={data.definition.id} label="the identity digest" length={16} />
					</span>
					<span class="inline-flex items-center gap-1">
						genesis <Digest
							value={data.definition.genesis}
							label="the genesis digest"
							length={16}
						/>
					</span>
					<span class="inline-flex items-center gap-1">
						current <Digest
							value={data.definition.current}
							label="the definition digest"
							length={16}
						/>
					</span>
				</div>
			</div>
		</section>

		{#if payload}
			<details class="collapse-arrow collapse border border-base-300 bg-base-200">
				<summary class="collapse-title inline-flex items-center gap-2 font-medium">
					<FileCode size={16} />
					Raw signed definition
				</summary>
				<div class="collapse-content">
					<div class="overflow-x-auto">
						<table class="table table-sm">
							<tbody>
								{#each Object.entries(payload) as [key, entry] (key)}
									<tr>
										<th scope="row" class="align-top font-mono">{key}</th>
										<td class="break-all">{value(entry)}</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				</div>
			</details>
		{/if}
	{/if}
</div>
