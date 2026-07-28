<script lang="ts">
	import { onMount } from 'svelte';
	import {
		addOrgMember,
		createOrgTeam,
		myOrgs,
		orgDetail,
		orgMembers,
		removeOrgMember,
		type OrgDetail,
		type OrgMember
	} from '$lib/api/orgs';
	import type { PageProps } from './$types';

	let { params }: PageProps = $props();

	let detail = $state<OrgDetail | null>(null);
	let members = $state<OrgMember[]>([]);
	let role = $state<string | null>(null);
	let error = $state<string | null>(null);
	let loading = $state(true);
	let email = $state('');
	let memberRole = $state('member');
	let teamName = $state('');
	let busy = $state(false);

	const canManage = $derived(role === 'owner' || role === 'admin');

	onMount(load);

	async function load() {
		loading = true;
		try {
			const [org, people, mine] = await Promise.all([orgDetail(params.handle), orgMembers(params.handle), myOrgs()]);
			detail = org;
			members = people;
			role = mine.find((entry) => entry.handle === org.handle)?.role ?? null;
			error = null;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not load the organization';
		} finally {
			loading = false;
		}
	}

	async function addMember(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		try {
			await addOrgMember(params.handle, email.trim(), memberRole);
			email = '';
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not add the member';
		} finally {
			busy = false;
		}
	}

	async function removeMember(member: OrgMember) {
		busy = true;
		try {
			await removeOrgMember(params.handle, member.user_id);
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not remove the member';
		} finally {
			busy = false;
		}
	}

	async function addTeam(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		try {
			await createOrgTeam(params.handle, teamName.trim());
			teamName = '';
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not create the team';
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head>
	<title>{detail?.display_name ?? params.handle} · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<h1 class="text-2xl font-bold">{detail?.display_name ?? params.handle}</h1>
	{#if role}
		<p class="text-base-content/80 text-sm">Your role here is {role}.</p>
	{/if}

	{#if error}
		<div role="alert" class="alert alert-error"><span>{error}</span></div>
	{/if}

	{#if loading}
		<span class="loading loading-spinner loading-sm" aria-hidden="true"></span>
		<span class="sr-only" role="status">Loading the organization</span>
	{:else if detail}
		<section class="card card-border">
			<div class="card-body">
				<h2 class="card-title">Members</h2>
				<div class="overflow-x-auto">
					<table class="table table-sm">
						<thead>
							<tr>
								<th scope="col">Account</th>
								<th scope="col">Role</th>
								<th scope="col"></th>
							</tr>
						</thead>
						<tbody>
							{#each members as member (member.user_id)}
								<tr>
									<td>{member.email}</td>
									<td><span class="badge badge-outline">{member.role}</span></td>
									<td>
										{#if canManage}
											<button
												class="btn btn-ghost btn-xs"
												type="button"
												disabled={busy}
												onclick={() => removeMember(member)}
												aria-label={`Remove ${member.email}`}
											>
												remove
											</button>
										{/if}
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
				{#if canManage}
					<form class="flex flex-wrap items-end gap-3" onsubmit={addMember}>
						<label class="form-control">
							<span class="label-text">Account email</span>
							<input class="input input-bordered" type="email" bind:value={email} required aria-label="Member email" />
						</label>
						<label class="form-control">
							<span class="label-text">Role</span>
							<select class="select select-bordered" bind:value={memberRole} aria-label="Member role">
								<option value="member">member</option>
								<option value="admin">admin</option>
								<option value="owner">owner</option>
							</select>
						</label>
						<button class="btn" type="submit" disabled={busy}>Add member</button>
					</form>
					<p class="text-base-content/60 text-sm">
						A role grant is not a shared login and does not hand over signing keys. The last owner
						cannot be removed.
					</p>
				{/if}
			</div>
		</section>

		<section class="card card-border">
			<div class="card-body">
				<h2 class="card-title">Teams</h2>
				{#if detail.teams.length === 0}
					<p class="text-base-content/60 text-sm">No teams yet.</p>
				{:else}
					<ul class="flex flex-wrap gap-2">
						{#each detail.teams as team (team.id)}
							<li class="badge badge-outline">{team.display_name}</li>
						{/each}
					</ul>
				{/if}
				{#if canManage}
					<form class="flex flex-wrap items-end gap-3" onsubmit={addTeam}>
						<label class="form-control">
							<span class="label-text">Team name</span>
							<input class="input input-bordered" bind:value={teamName} required aria-label="Team name" />
						</label>
						<button class="btn" type="submit" disabled={busy}>Create team</button>
					</form>
				{/if}
			</div>
		</section>
	{/if}
</div>
