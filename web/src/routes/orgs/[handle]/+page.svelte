<script lang="ts">
	import { onMount } from 'svelte';
	import { FolderTree, MoveRight, Plus, Trash2, UserPlus } from '@lucide/svelte';
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
	import Avatar from '$lib/components/Avatar.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import PageHeader from '$lib/components/PageHeader.svelte';
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
	<PageHeader title={detail?.display_name ?? params.handle} subtitle={`@${params.handle}`}>
		{#if role}
			<span class="badge badge-outline">Your role: {role}</span>
		{/if}
	</PageHeader>

	{#if error}
		<div role="alert" class="alert alert-error"><span>{error}</span></div>
	{/if}

	{#if loading}
		<div class="skeleton h-24 w-full"></div>
	{:else if detail}
		<section class="card card-border bg-base-200">
			<div class="card-body gap-4">
				<h2 class="card-title"><UserPlus size={18} /> Members</h2>
				{#if members.length === 0}
					<EmptyState title="No members yet" />
				{:else}
					<ul class="flex flex-col divide-y divide-base-300">
						{#each members as member (member.user_id)}
							<li class="flex items-center gap-3 py-2">
								<Avatar name={member.email} id={member.user_id} size={36} />
								<span class="min-w-0 flex-1 truncate">{member.email}</span>
								<span class="badge badge-outline badge-sm">{member.role}</span>
								{#if canManage}
									<button
										class="btn btn-ghost btn-xs"
										type="button"
										disabled={busy}
										onclick={() => removeMember(member)}
										aria-label={`Remove ${member.email}`}
									>
										<Trash2 size={13} />
										remove
									</button>
								{/if}
							</li>
						{/each}
					</ul>
				{/if}
				{#if canManage}
					<form class="flex flex-wrap items-end gap-3" onsubmit={addMember}>
						<label class="floating-label">
							<span>Account email</span>
							<input
								class="input w-64"
								type="email"
								bind:value={email}
								required
								aria-label="Member email"
							/>
						</label>
						<label class="floating-label">
							<span>Role</span>
							<select class="select w-32" bind:value={memberRole} aria-label="Member role">
								<option value="member">member</option>
								<option value="admin">admin</option>
								<option value="owner">owner</option>
							</select>
						</label>
						<button class="btn" type="submit" disabled={busy}>
							<Plus size={16} />
							Add member
						</button>
					</form>
					<p class="text-xs text-base-content/50">
						A role grant is not a shared login and does not hand over signing keys. The last owner
						cannot be removed.
					</p>
				{/if}
			</div>
		</section>

		<section class="card card-border bg-base-200">
			<div class="card-body gap-4">
				<h2 class="card-title"><FolderTree size={18} /> Teams</h2>
				{#if detail.teams.length === 0}
					<p class="text-sm text-base-content/60">No teams yet.</p>
				{:else}
					{#snippet teamNode(team: OrgTeam, depth: number)}
						<li style={`margin-inline-start: ${depth}rem`}>
							<div class="flex flex-wrap items-center gap-2">
								<span class="badge badge-outline">{team.display_name}</span>
								{#if canManage}
									{#if moving === team.id}
										<select class="select select-xs" bind:value={moveTarget}>
											<option value="">top level</option>
											{#each movableTeams.filter((candidate) => candidate.id !== team.id) as candidate (candidate.id)}
												<option value={candidate.id}>{candidate.display_name}</option>
											{/each}
										</select>
										<button class="btn btn-xs" onclick={() => moveTeam(team)}>
											<MoveRight size={13} />
											Move
										</button>
										<button class="btn btn-xs btn-ghost" onclick={() => (moving = null)}
											>Cancel</button
										>
									{:else}
										<button class="btn btn-xs btn-ghost" onclick={() => (moving = team.id)}>
											<MoveRight size={13} />
											Move
										</button>
									{/if}
								{/if}
							</div>
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
						<label class="floating-label">
							<span>Team name</span>
							<input class="input w-56" bind:value={teamName} required aria-label="Team name" />
						</label>
						<label class="floating-label">
							<span>Parent team</span>
							<select class="select w-56" bind:value={parentTeamId} aria-label="Parent team">
								<option value="">none</option>
								{#each detail.teams as team (team.id)}
									<option value={team.id}>{team.display_name}</option>
								{/each}
							</select>
						</label>
						<button class="btn" type="submit" disabled={busy}>
							<Plus size={16} />
							Create team
						</button>
					</form>
				{/if}
			</div>
		</section>
	{/if}
</div>
