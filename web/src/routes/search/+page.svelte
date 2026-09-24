<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import MultiSelectField from '$lib/components/MultiSelectField.svelte';
	import PageHeader from '$lib/components/PageHeader.svelte';
	import ProjectCard from '$lib/components/ProjectCard.svelte';
	import SearchBar from '$lib/components/SearchBar.svelte';
	import SelectField, { type SelectOption } from '$lib/components/SelectField.svelte';
	import { shortDigest } from '$lib/api/digests';
	import type { FacetValue } from '$lib/api/search';
	import { home } from '$lib/home.svelte';
	import { pageTitle } from '$lib/title.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	function countFor(facets: FacetValue[], value: string): number | null {
		return facets.find((facet) => facet.value === value)?.count ?? null;
	}

	function withCounts(values: FacetValue[], options: SelectOption[]): SelectOption[] {
		return options.map((option) => {
			const count = countFor(values, option.value);
			return count === null ? option : { ...option, label: `${option.label} (${count})` };
		});
	}

	const gameOptions = $derived<SelectOption[]>([
		{ value: '', label: 'All games' },
		...data.games.map((game) => ({
			value: game.id,
			label: game.display_name ?? shortDigest(game.id),
		})),
	]);

	const loaderOptions = $derived<SelectOption[]>([
		{ value: '', label: 'Any loader' },
		...data.loaders.map((loader) => ({
			value: loader.id,
			label: loader.display_name ?? shortDigest(loader.id),
		})),
	]);

	const versionOptions = $derived<SelectOption[]>(
		data.gamePayload?.version_catalog?.length
			? data.gamePayload.version_catalog.map((version) => {
					const count = countFor(data.facets.game_version, version);
					return { value: version, label: count === null ? version : `${version} (${count})` };
				})
			: withCounts(
					data.facets.game_version,
					data.facets.game_version.map((facet) => ({ value: facet.value, label: facet.value })),
				),
	);

	const categoryOptions = $derived<SelectOption[]>(
		data.gamePayload?.categories?.length
			? data.gamePayload.categories.map((category) => ({
					value: category.id,
					label: category.label,
				}))
			: data.facets.category.map((facet) => ({ value: facet.value, label: facet.value })),
	);

	const tagOptions = $derived<SelectOption[]>(
		data.gamePayload?.tags?.length
			? data.gamePayload.tags.map((tag) => ({ value: tag.id, label: tag.label }))
			: data.facets.tag.map((facet) => ({ value: facet.value, label: facet.value })),
	);

	const channelOptions = $derived<SelectOption[]>([
		{ value: '', label: 'Any channel' },
		...withCounts(
			data.facets.channel,
			data.facets.channel.length
				? data.facets.channel.map((facet) => ({ value: facet.value, label: facet.value }))
				: ['release', 'beta', 'alpha'].map((value) => ({ value, label: value })),
		),
	]);

	const sortOptions: SelectOption[] = [
		{ value: '', label: 'Best match' },
		{ value: 'updated', label: 'Recently updated' },
		{ value: 'created', label: 'Newest' },
		{ value: 'name', label: 'Name' },
		{ value: 'popularity', label: 'Popularity' },
	];

	const selectedVersions = $derived(data.gameVersion ? data.gameVersion.split(',') : []);

	const gameNames = $derived(
		new Map(data.games.map((game) => [game.id, game.display_name ?? shortDigest(game.id)])),
	);

	const hasFilters = $derived(
		[data.q, data.game, data.loader, data.category, data.tag, data.gameVersion, data.channel].some(
			(value) => value.trim().length > 0,
		),
	);

	function apply(updates: Record<string, string>) {
		const params = new URLSearchParams(page.url.searchParams);
		for (const [key, value] of Object.entries(updates)) {
			if (value) {
				params.set(key, value);
			} else {
				params.delete(key);
			}
		}
		goto(home.url(`/search?${params.toString()}`), { keepFocus: true, noScroll: true });
	}

	function clearAll() {
		goto(home.url('/search'), { keepFocus: true, noScroll: true });
	}
</script>

<svelte:head>
	<title>{pageTitle('Mods')}</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<PageHeader
		title="Mods"
		subtitle="Search across every project this instance indexes, then narrow it down."
	/>

	<SearchBar value={data.q} />

	<div class="grid gap-6 lg:grid-cols-[280px_1fr]">
		<aside class="lg:sticky lg:top-24 lg:self-start">
			<div class="card card-border bg-base-200">
				<div class="card-body gap-1 p-4">
					<div class="flex items-center justify-between">
						<h2 class="font-semibold">Filters</h2>
						{#if hasFilters}
							<button class="btn btn-ghost btn-xs" type="button" onclick={clearAll}>Clear</button>
						{/if}
					</div>

					<SelectField
						label="Game"
						value={data.game}
						options={gameOptions}
						placeholder="All games"
						onchange={(value) =>
							apply({ game: value, loader: '', category: '', tag: '', game_version: '' })}
					/>
					<SelectField
						label="Loader"
						value={data.loader}
						options={loaderOptions}
						placeholder="Any loader"
						onchange={(value) => apply({ loader: value })}
					/>
					<SelectField
						label="Sort by"
						value={data.sort}
						options={sortOptions}
						placeholder="Best match"
						onchange={(value) => apply({ sort: value })}
					/>
					<MultiSelectField
						label="Game version"
						value={selectedVersions}
						options={versionOptions}
						placeholder="Any version"
						onchange={(value) => apply({ game_version: value.join(',') })}
					/>
					<SelectField
						label="Category"
						value={data.category}
						options={[{ value: '', label: 'Any category' }, ...categoryOptions]}
						placeholder="Any category"
						onchange={(value) => apply({ category: value })}
					/>
					<SelectField
						label="Tag"
						value={data.tag}
						options={[{ value: '', label: 'Any tag' }, ...tagOptions]}
						placeholder="Any tag"
						onchange={(value) => apply({ tag: value })}
					/>
					<SelectField
						label="Channel"
						value={data.channel}
						options={channelOptions}
						placeholder="Any channel"
						onchange={(value) => apply({ channel: value })}
					/>
				</div>
			</div>
		</aside>

		<section class="flex flex-col gap-4">
			{#if data.error}
				<div role="alert" class="alert alert-error"><span>{data.error}</span></div>
			{/if}

			{#if !hasFilters}
				<EmptyState
					title="Start with a search or a filter"
					message="Type a name, or pick a game on the left. Results show every matching project this instance can reach."
				/>
			{:else if data.results.length === 0}
				<EmptyState
					title="No mods match"
					message="Try a broader search, or remove a filter or two."
				/>
			{:else}
				<p class="text-sm text-base-content/60">
					{data.results.length}
					{data.results.length === 1 ? 'result' : 'results'}
				</p>
				<div class="flex flex-col gap-3">
					{#each data.results as result (result.project_id)}
						<ProjectCard {result} gameName={gameNames.get(result.game_id) ?? ''} />
					{/each}
				</div>
			{/if}
		</section>
	</div>
</div>
