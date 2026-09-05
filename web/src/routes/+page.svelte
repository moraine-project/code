<script lang="ts">
	import { goto } from '$app/navigation';
	import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';
	import { lookupDigest, shortDigest, type DigestLookup } from '$lib/api/registry';
	import Digest from '$lib/components/Digest.svelte';

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

<div class="flex flex-col gap-12">
	<section class="flex flex-col items-start gap-5">
		<h1 class="max-w-3xl text-3xl font-bold tracking-tight sm:text-4xl">
			Game mods, published by the people who made them
		</h1>
		<p class="max-w-2xl text-base-content/80">
			Moraine is a registry that any community can run. Every release is signed by its publisher, so
			you can see who published a file and whether it has changed since. No single company decides
			what is allowed to exist, and a project stays reachable at its own home even when a site stops
			recommending it.
		</p>
		<div class="flex flex-wrap gap-3">
			<a class="btn btn-primary" href="/search">Find a mod</a>
			<a class="btn" href="/publish">Publish a mod</a>
			<a class="btn btn-ghost" href="/about">How this works</a>
		</div>
	</section>

	<section class="flex flex-col gap-4">
		<h2 class="text-lg font-semibold">How it works</h2>
		<ul class="steps steps-vertical sm:steps-horizontal">
			<li class="step step-primary">
				<span class="mt-1 text-left text-sm">
					<strong class="block">Find a game</strong>
					Browse the games and loaders this instance knows about.
				</span>
			</li>
			<li class="step step-primary">
				<span class="mt-1 text-left text-sm">
					<strong class="block">Open a mod</strong>
					Read what it does and pick the release that matches your game version.
				</span>
			</li>
			<li class="step step-primary">
				<span class="mt-1 text-left text-sm">
					<strong class="block">Check the file</strong>
					Download it, then confirm the fingerprint matches before you run it.
				</span>
			</li>
		</ul>
		<p class="text-sm text-base-content/70">
			New to this? Start with <a class="link link-hover" href="/games">Browse</a>, or read
			<a class="link link-hover" href="/about">how a home works</a> and
			<a class="link link-hover" href="/security">what a signature does and does not prove</a>.
		</p>
	</section>

	<section class="grid gap-4 sm:grid-cols-3">
		<div class="card card-border bg-base-200">
			<div class="card-body">
				<h2 class="card-title text-base">Signed by the publisher</h2>
				<p class="text-base-content/80 text-sm">
					A release is tied to a key the project controls. An instance can host or hide a project,
					but it cannot forge a release or quietly change one.
				</p>
			</div>
		</div>
		<div class="card card-border bg-base-200">
			<div class="card-body">
				<h2 class="card-title text-base">Hosted by your community</h2>
				<p class="text-base-content/80 text-sm">
					A directory chooses what it recommends. Removing a project from one list does not erase
					it, because the publisher's home keeps serving the signed files.
				</p>
			</div>
		</div>
		<div class="card card-border bg-base-200">
			<div class="card-body">
				<h2 class="card-title text-base">You can check it yourself</h2>
				<p class="text-base-content/80 text-sm">
					This page shows what a home claims. The verifier tool checks the actual bytes on your
					machine, so you never have to take a website's word for it.
				</p>
			</div>
		</div>
	</section>

	<section class="grid gap-6 lg:grid-cols-2">
		<div class="card card-border bg-base-200">
			<div class="card-body gap-4">
				<h2 class="card-title">Open a project you were given</h2>
				<p class="text-base-content/80 text-sm">
					Every project has a stable ID and a home address. If someone sent you both, you can open
					the project here even when no directory lists it.
				</p>
				<form class="flex flex-col gap-2" onsubmit={resolve}>
					<label class="label" for="project-id">Project ID</label>
					<input
						id="project-id"
						class="input w-full font-mono text-sm"
						bind:value={projectId}
						placeholder="gd:sha256:…"
					/>
					<label class="label" for="project-home">Home address</label>
					<input
						id="project-home"
						class="input w-full"
						bind:value={home}
						placeholder="https://home.example"
					/>
					<button class="btn btn-primary mt-1 self-start" type="submit">Open project</button>
				</form>
			</div>
		</div>

		<div class="card card-border bg-base-200">
			<div class="card-body gap-4">
				<h2 class="card-title">Check a file you already downloaded</h2>
				<p class="text-base-content/80 text-sm">
					A file's SHA-256 fingerprint points back to the release that published it. Compute it with
					your own tools, then paste it here to see where the file came from.
				</p>
				<form class="flex flex-col gap-2" onsubmit={lookup}>
					<label class="label" for="artifact-digest">File fingerprint (SHA-256)</label>
					<input
						id="artifact-digest"
						class="input w-full font-mono text-sm"
						bind:value={digest}
						placeholder="sha256:…"
					/>
					<button class="btn mt-1 self-start" type="submit" disabled={looking}>
						{looking ? 'Looking…' : 'Find the release'}
					</button>
				</form>
				{#if lookupError}
					<div role="alert" class="alert alert-error alert-soft"><span>{lookupError}</span></div>
				{:else if lookupResult}
					{#if lookupResult.matches.length === 0}
						<p class="text-base-content/80 text-sm">
							No release on this home publishes that fingerprint.
						</p>
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
									<Digest copyOnly value={match.project_id} label="the project id" />
									{#if match.human_version}
										<span class="badge badge-ghost">{match.human_version}</span>
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
		</div>
	</section>
</div>
