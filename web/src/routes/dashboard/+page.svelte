<script lang="ts">
	import { PUBLIC_MORAINE_REGISTRY } from '$env/static/public';
	import { Server } from '@lucide/svelte';
	import Accounts from '$lib/components/dashboard/Accounts.svelte';
	import Definitions from '$lib/components/dashboard/Definitions.svelte';
	import Federation from '$lib/components/dashboard/Federation.svelte';
	import Overview from '$lib/components/dashboard/Overview.svelte';
	import Policy from '$lib/components/dashboard/Policy.svelte';
	import Moderation from '$lib/components/dashboard/Moderation.svelte';
	import Records from '$lib/components/dashboard/Records.svelte';
	import EmptyState from '$lib/components/EmptyState.svelte';
	import PageHeader from '$lib/components/PageHeader.svelte';
	import { session } from '$lib/session.svelte';

	type Tab =
		| 'overview'
		| 'accounts'
		| 'definitions'
		| 'federation'
		| 'policy'
		| 'moderation'
		| 'records';
	let tab = $state<Tab>('overview');

	const isOperator = $derived(session.user?.role === 'operator');
</script>

<svelte:head>
	<title>Dashboard · Moraine</title>
</svelte:head>

<div class="flex flex-col gap-6">
	<PageHeader
		title="Dashboard"
		subtitle="Run this instance: accounts, definitions, and federation."
	/>

	{#if !session.user}
		<EmptyState title="Sign in" message="The dashboard is for operators.">
			<a class="btn btn-primary" href="/account">Sign in</a>
		</EmptyState>
	{:else if !isOperator}
		<EmptyState
			title="You are not an operator"
			message="Your account is a member. Ask an operator for access, or use the site normally."
		>
			<a class="btn btn-primary" href="/">Back home</a>
		</EmptyState>
	{:else}
		<div role="tablist" class="tabs tabs-box w-fit">
			<button
				role="tab"
				class="tab"
				class:tab-active={tab === 'overview'}
				onclick={() => (tab = 'overview')}
			>
				Overview
			</button>
			<button
				role="tab"
				class="tab"
				class:tab-active={tab === 'accounts'}
				onclick={() => (tab = 'accounts')}
			>
				Accounts
			</button>
			<button
				role="tab"
				class="tab"
				class:tab-active={tab === 'definitions'}
				onclick={() => (tab = 'definitions')}
			>
				Definitions
			</button>
			<button
				role="tab"
				class="tab"
				class:tab-active={tab === 'federation'}
				onclick={() => (tab = 'federation')}
			>
				Federation
			</button>
			<button
				role="tab"
				class="tab"
				class:tab-active={tab === 'policy'}
				onclick={() => (tab = 'policy')}>Policy</button
			>
			<button
				role="tab"
				class="tab"
				class:tab-active={tab === 'moderation'}
				onclick={() => (tab = 'moderation')}>Moderation</button
			>
			<button
				role="tab"
				class="tab"
				class:tab-active={tab === 'records'}
				onclick={() => (tab = 'records')}>Records</button
			>
		</div>

		<section class="card card-border bg-base-200">
			<div class="card-body">
				{#if tab === 'overview'}
					<Overview />
				{:else if tab === 'accounts'}
					<Accounts />
				{:else if tab === 'definitions'}
					<Definitions />
				{:else if tab === 'federation'}
					<Federation />
				{:else if tab === 'policy'}
					<Policy />
				{:else if tab === 'moderation'}
					<Moderation />
				{:else}
					<Records />
				{/if}
			</div>
		</section>

		<p class="flex items-center gap-2 text-xs text-base-content/50">
			<Server size={14} />
			{#if PUBLIC_MORAINE_REGISTRY}
				Resolves through <strong>{PUBLIC_MORAINE_REGISTRY}</strong> unless a page is given another home.
			{:else}
				No default home is baked in.
			{/if}
		</p>
	{/if}
</div>
