<script lang="ts">
	import { onMount } from 'svelte';
	import {
		addOrgMember,
		createOrgTeam,
		myOrgs,
		orgDetail,
		orgMembers,
		removeOrgMember,
		reparentOrgTeam,
		type OrgDetail,
		type OrgMember,
		type OrgTeam,
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
	let parentTeamId = $state('');
	let moving = $state<string | null>(null);
	let moveTarget = $state('');
	let busy = $state(false);

	const canManage = $derived(role === 'owner' || role === 'admin');
	const rootTeams = $derived(detail?.teams.filter((team) => !team.parent_team_id) ?? []);
	const childrenOf = (parentId: string) =>
		detail?.teams.filter((team) => team.parent_team_id === parentId) ?? [];
	const movableTeams = $derived(detail?.teams ?? []);

	onMount(load);

	async function load() {
		loading = true;
		try {
			const [org, people, mine] = await Promise.all([
				orgDetail(params.handle),
				orgMembers(params.handle),
				myOrgs(),
			]);
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

	async function moveTeam(team: OrgTeam) {
		moving = team.id;
		try {
			await reparentOrgTeam(params.handle, team.id, moveTarget || null);
			moveTarget = '';
			await load();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'could not move the team';
		} finally {
			moving = null;
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
			await createOrgTeam(params.handle, teamName.trim(), parentTeamId || null);
			teamName = '';
			parentTeamId = '';
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
						<fieldset class="fieldset">
							<legend class="fieldset-legend">Account email</legend>
							<input
								class="input"
								type="email"
								bind:value={email}
								required
								aria-label="Member email"
							/>
						</fieldset>
						<fieldset class="fieldset">
							<legend class="fieldset-legend">Role</legend>
							<select class="select" bind:value={memberRole} aria-label="Member role">
								<option value="member">member</option>
								<option value="admin">admin</option>
								<option value="owner">owner</option>
							</select>
						</fieldset>
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
					{#snippet teamNode(team: OrgTeam, depth: number)}
						<li style={`margin-inline-start: ${depth}rem`}>
							<span class="badge badge-outline">{team.display_name}</span>
							{#if canManage}
								{#if moving === team.id}
									<select class="select select-xs" bind:value={moveTarget}>
										<option value="">top level</option>
										{#each movableTeams.filter((candidate) => candidate.id !== team.id) as candidate (candidate.id)}
											<option value={candidate.id}>{candidate.display_name}</option>
										{/each}
									</select>
									<button class="btn btn-xs" onclick={() => moveTeam(team)}>Move</button>
									<button class="btn btn-xs btn-ghost" onclick={() => (moving = null)}
										>Cancel</button
									>
								{:else}
									<button class="btn btn-xs btn-ghost" onclick={() => (moving = team.id)}
										>Move</button
									>
								{/if}
							{/if}
							{#if childrenOf(team.id).length > 0}
								<ul class="mt-2 flex flex-wrap gap-2">
									{#each childrenOf(team.id) as child (child.id)}
										{@render teamNode(child, depth + 1)}
									{/each}
								</ul>
							{/if}
						</li>
					{/snippet}
					<ul class="flex flex-wrap gap-2">
						{#each rootTeams as team (team.id)}
							{@render teamNode(team, 0)}
						{/each}
					</ul>
				{/if}
				{#if canManage}
					<form class="flex flex-wrap items-end gap-3" onsubmit={addTeam}>
						<fieldset class="fieldset">
							<legend class="fieldset-legend">Team name</legend>
							<input class="input" bind:value={teamName} required aria-label="Team name" />
						</fieldset>
						<fieldset class="fieldset">
							<legend class="fieldset-legend">Parent team</legend>
							<select class="select" bind:value={parentTeamId} aria-label="Parent team">
								<option value="">none</option>
								{#each detail.teams as team (team.id)}
									<option value={team.id}>{team.display_name}</option>
								{/each}
							</select>
						</fieldset>
						<button class="btn" type="submit" disabled={busy}>Create team</button>
					</form>
				{/if}
			</div>
		</section>
	{/if}
</div>
