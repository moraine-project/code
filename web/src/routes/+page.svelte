<script lang="ts">
	import { goto } from '$app/navigation';
	import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';
	import { lookupDigest, shortDigest, type DigestLookup } from '$lib/api/registry';

	let projectId = $state('');
	let home = $state(PUBLIC_MORAINE_REGISTRY ?? 'http://127.0.0.1:8080');
	let digest = $state('');
	let lookupResult = $state<DigestLookup | null>(null);
	let lookupError = $state<string | null>(null);
	let looking = $state(false);

	function resolve(event: SubmitEvent) {
		event.preventDefault();
		const project = projectId.trim();
		if (project.length === 0) {
			return;
		}
		goto(`/p/${encodeURIComponent(project)}?home=${encodeURIComponent(home.trim())}`);
	}

	async function lookup(event: SubmitEvent) {
		event.preventDefault();
		lookupResult = null;
		lookupError = null;
		looking = true;
		try {
			lookupResult = await lookupDigest(home, digest);
		} catch (cause) {
			lookupError = cause instanceof Error ? cause.message : 'the lookup failed';
		} finally {
			looking = false;
		}
	}
</script>

<div class="flex flex-col gap-10">
	<section class="flex flex-col gap-4">
		<h1 class="text-3xl font-bold tracking-tight sm:text-4xl">Every project has a home</h1>
		<p class="text-base-content/80 max-w-2xl">
			A mod lives at a home its publisher controls. Releases are signed and immutable, and anyone
			can resolve them directly. Paste a project ID and the home that serves it; this page fetches
			the signed records from that home and shows exactly what it found.
		</p>
	</section>

	<section class="card card-border bg-base-200">
		<div class="card-body">
			<h2 class="card-title">Resolve a project</h2>
			<form class="join w-full" onsubmit={resolve}>
				<input
					class="input join-item w-full max-w-xs"
					bind:value={home}
					aria-label="Home registry URL"
					placeholder="https://home.example"
				/>
				<input
					class="input join-item flex-1"
					bind:value={projectId}
					aria-label="Project ID"
					placeholder="gd:sha256:..."
				/>
				<button class="btn join-item" type="submit">Resolve</button>
			</form>
		</div>
	</section>

	<section class="card card-border bg-base-200">
		<div class="card-body">
			<h2 class="card-title">Find a release by file digest</h2>
			<p class="text-base-content/80 text-sm">
				Have a JAR or ZIP of unknown origin? Its SHA-256 points back to the release that
				published it.
			</p>
			<form class="join w-full" onsubmit={lookup}>
				<input
					class="input join-item flex-1"
					bind:value={digest}
					aria-label="Artifact SHA-256"
					placeholder="sha256:..."
				/>
				<button class="btn join-item" type="submit" disabled={looking}>
					{looking ? 'Looking…' : 'Look up'}
				</button>
			</form>
			{#if lookupError}
				<div role="alert" class="alert alert-error"><span>{lookupError}</span></div>
			{:else if lookupResult}
				{#if lookupResult.matches.length === 0}
					<p class="text-base-content/80 text-sm">No release publishes that digest on this home.</p>
				{:else}
					<ul class="flex flex-col gap-2">
						{#each lookupResult.matches as match (match.release)}
							<li class="flex flex-wrap items-center gap-2 text-sm">
								<a
									class="link link-hover font-mono"
									href={`/p/${encodeURIComponent(match.project_id)}?home=${encodeURIComponent(home.trim())}`}
								>
									{shortDigest(match.project_id)}
								</a>
								{#if match.human_version}
									<span class="badge badge-outline">{match.human_version}</span>
								{/if}
								{#if match.filename}
									<span class="text-base-content/60">{match.filename}</span>
								{/if}
							</li>
						{/each}
					</ul>
				{/if}
			{/if}
		</div>
	</section>

	<section class="grid gap-4 sm:grid-cols-3">
		<div class="card card-border">
			<div class="card-body">
				<h3 class="card-title text-base">Signed by the publisher</h3>
				<p class="text-base-content/80 text-sm">
					A release is addressed by digest and signed by a key the project's root authorized. A
					home cannot forge it.
				</p>
			</div>
		</div>
		<div class="card card-border">
			<div class="card-body">
				<h3 class="card-title text-base">Indexed, not owned</h3>
				<p class="text-base-content/80 text-sm">
					A directory chooses what it lists. Delisting never erases the project, because the home
					keeps serving the signed bytes.
				</p>
			</div>
		</div>
		<div class="card card-border">
			<div class="card-body">
				<h3 class="card-title text-base">Verified on your machine</h3>
				<p class="text-base-content/80 text-sm">
					This page shows what the home claims. Independent verification happens with the
					verifier CLI, on the bytes you actually download.
				</p>
			</div>
		</div>
	</section>
</div>
