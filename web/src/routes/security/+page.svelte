<script lang="ts">
	import PageHeader from '$lib/components/PageHeader.svelte';
	import { pageTitle } from '$lib/title.svelte';
</script>

<svelte:head>
	<title>{pageTitle('Security')}</title>
</svelte:head>

<div class="flex max-w-3xl flex-col gap-6">
	<PageHeader
		title="What security here does and does not mean"
		subtitle="A signature proves authorization. A digest proves the bytes. Neither proves the software is safe."
	/>

	<p class="text-base-content/80">
		Moraine is built so that the important claims are checkable rather than promised. The honest
		summary is that a signature proves authorization, a digest proves the bytes, and neither proves
		the software is safe.
	</p>

	<h2 class="text-xl font-semibold">Signatures prove authorization</h2>
	<p class="text-base-content/80">
		A project's records are signed by the keys the project published at creation. A valid signature
		means the object was authorized by whoever holds those keys. It does not mean the author is
		trustworthy, honest, or the person a name suggests, and it does not mean the content is
		harmless.
	</p>
	<p class="text-base-content/80">
		A changed display name or a newly delegated signing key is part of that project's history. It is
		shown as a change rather than smoothed over, because the keys that authorize a release are the
		relevant fact, not the name attached to it.
	</p>

	<h2 class="text-xl font-semibold">Digests prove the bytes</h2>
	<p class="text-base-content/80">
		Every artifact is addressed by the SHA-256 of its contents. If the digest of what you downloaded
		matches what the release declared, the file is exactly the file that was published, byte for
		byte. A partial download cannot be verified; the full stream is needed.
	</p>

	<h2 class="text-xl font-semibold">Reviews are local and optional</h2>
	<p class="text-base-content/80">
		A registry may review submissions before listing them, or list them immediately. A review is
		that registry's own decision and is not a certification by the project or by any other registry.
		An unlisted or blocked project is not deleted: its records remain, and its direct home URL still
		resolves.
	</p>

	<h2 class="text-xl font-semibold">Withdrawal is a statement, not erasure</h2>
	<p class="text-base-content/80">
		When a release is withdrawn, the withdrawal is recorded as its own signed object and shown
		beside the release. The original release record is left intact so the history stays auditable.
		Artifact bytes may later be collected by the home under its retention policy, which makes the
		file unavailable without changing what was published.
	</p>
</div>
