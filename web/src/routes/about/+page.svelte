<script lang="ts">
	import PageHeader from '$lib/components/PageHeader.svelte';
</script>

<svelte:head>
	<title>How it works · Moraine</title>
</svelte:head>

<div class="flex max-w-3xl flex-col gap-6">
	<PageHeader
		title="How it works"
		subtitle="Projects live at homes their publishers control, and directories index them. Nothing here depends on one central server."
	/>

	<p class="text-base-content/80">
		Moraine does not have one central server that every project must live on. A project is published
		by a <strong>home</strong>, the registry that holds its objects and artifacts. The project's
		identity is a signed record, not a name on a particular website, so the same project can be
		served by more than one home without becoming a different project.
	</p>

	<h2 class="text-xl font-semibold">Home URLs</h2>
	<p class="text-base-content/80">
		A home is addressed by its URL. When you enter a home URL next to a project, this site does not
		trust that home's copy of the project blindly: it fetches the project's signed records and
		verifies them against the identity the project was created with. If the bytes do not verify, the
		copy is rejected.
	</p>
	<p class="text-base-content/80">
		This is what a direct home URL buys you. You can reach a project through a directory that
		indexed it, or go straight to the home, and both paths have to satisfy the same signature
		checks. Search is a convenience; the record is the authority.
	</p>

	<h2 class="text-xl font-semibold">Directories</h2>
	<p class="text-base-content/80">
		A directory is a registry that indexes and reviews projects from many homes. It decides what to
		list, what to retain, and whether publication is immediate or reviewed before it appears. Its
		index is disposable and rebuildable, never a property of the protocol. Two directories can
		reasonably disagree about whether to list the same project.
	</p>

	<h2 class="text-xl font-semibold">What a home is not</h2>
	<p class="text-base-content/80">
		A home URL is not a username and password, and following a home does not hand it authority over
		your copy. A home can go offline or stop serving a project, which makes its artifacts
		unavailable there; it cannot rewrite a signed record into a different one.
	</p>
</div>
