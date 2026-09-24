<script lang="ts">
	import PageHeader from '$lib/components/PageHeader.svelte';
	import { pageTitle } from '$lib/title.svelte';
</script>

<svelte:head><title>{pageTitle('Instance information')}</title></svelte:head>

<div class="flex flex-col gap-6">
	<PageHeader title="Instance information" subtitle="How this Moraine home is operated." />
	<section class="card card-border bg-base-200">
		<div class="card-body gap-3">
			<h2 class="card-title">What this home does</h2>
			<p>
				This instance hosts project pages, signed releases, and the files its operator chooses to
				keep available. Independent directories and mirrors may make different decisions.
			</p>
			<h2 class="card-title mt-4">Moderation</h2>
			<p>
				Publication review and local policy decisions apply to this home. A review decision is not a
				claim that software is safe, and a signature proves authorization rather than harmlessness.
			</p>
			<h2 class="card-title mt-4">Contact</h2>
			<p>
				For moderation, legal, or security reports, contact the operator who gave you this instance
				URL.
			</p>
		</div>
	</section>
</div>
