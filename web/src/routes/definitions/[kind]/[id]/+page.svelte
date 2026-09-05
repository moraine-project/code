<script lang="ts">
	import Digest from '$lib/components/Digest.svelte';
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
	<a class="link link-hover w-fit" href="/games">← Back to browse</a>

	{#if data.error}
		<div role="alert" class="alert alert-error"><span>{data.error}</span></div>
	{:else if data.definition}
		<section class="card card-border">
			<div class="card-body">
				<h1 class="card-title">{data.definition.display_name ?? 'Unnamed'}</h1>
				<div class="flex flex-wrap items-center gap-2">
					<span class="badge badge-outline">{data.definition.kind}</span>
					<Digest value={data.definition.id} label="the identity digest" />
				</div>
				<p class="max-w-2xl text-base-content/80 text-sm">
					This is the current signed description of {data.definition.display_name ?? 'this entry'}.
					Installers use it to know which versions exist, how they are ordered, and how to set mods
					up. It is trusted because it verifies against the identity it was created with, not
					because this instance lists it.
				</p>
				<div class="text-base-content/60 flex flex-wrap gap-3 text-sm">
					<span
						>genesis <Digest
							value={data.definition.genesis}
							label="the genesis digest"
							length={12}
						/></span
					>
					<span
						>current <Digest
							value={data.definition.current}
							label="the definition digest"
							length={12}
						/></span
					>
				</div>
			</div>
		</section>

		{#if payload}
			<section class="card card-border bg-base-200">
				<div class="card-body">
					<details>
						<summary class="cursor-pointer font-medium">Raw signed definition</summary>
						<div class="mt-3 overflow-x-auto">
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
					</details>
				</div>
			</section>
		{/if}
	{/if}
</div>
