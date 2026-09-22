import { readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, test, type Page } from '@playwright/test';

const state = JSON.parse(readFileSync(join(tmpdir(), 'moraine-e2e.json'), 'utf8')) as {
	baseURL: string;
	password: string;
};

async function signIn(page: Page, password: string) {
	await page.goto('/account');
	await page.getByLabel('Email').fill('ops@example.org');
	await page.getByLabel('Password').fill(password);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await expect(page.getByRole('button', { name: 'Sign out' })).toBeVisible();
}

test.describe.configure({ mode: 'serial' });

test('renders the public routes and navigation surfaces', async ({ page }) => {
	for (const route of [
		'/about',
		'/security',
		'/instance',
		'/packs/00',
		'/games',
		'/search',
		'/account',
	]) {
		await page.goto(route);
		await expect(page.locator('main')).toBeVisible();
		await expect(page.locator('body')).not.toContainText('Internal Server Error');
	}

	await page.goto('/search');
	await page.getByRole('button', { name: 'Game', exact: true }).click();
	await page.getByRole('option', { name: 'Minecraft' }).click();
	await expect(page).toHaveURL(/game=/);
	await page.getByLabel('Loader').click();
	await page.getByRole('option', { name: 'Fabric', exact: true }).click();
	await expect(page).toHaveURL(/loader=/);
	await page.getByLabel('Sort by').click();
	await page.getByRole('option', { name: 'Recently updated' }).click();
	await expect(page).toHaveURL(/sort=updated/);
	await page.getByLabel('Game version').click();
	await page.getByRole('option', { name: '1.20.1' }).click();
	await expect(page).toHaveURL(/game_version=1.20.1/);
	await page.getByLabel('Category').click();
	await page.getByRole('option', { name: 'Adventure' }).click();
	await expect(page).toHaveURL(/category=adventure/);
	await page.getByLabel('Channel').click();
	await page.getByRole('option', { name: 'release' }).click();
	await expect(page).toHaveURL(/channel=release/);
	await page.getByRole('button', { name: 'Clear' }).click();
	await expect(page).toHaveURL(/\/search$/);
	for (const [label, query] of [
		['Newest', 'created'],
		['Name', 'name'],
		['Popularity', 'popularity'],
	] as const) {
		await page.getByLabel('Sort by').click();
		await page.getByRole('option', { name: label, exact: true }).click();
		await expect(page).toHaveURL(new RegExp(`sort=${query}`));
	}

	await page.goto('/games');
	const minecraft = page.getByRole('link', { name: /Minecraft/ }).first();
	const gameHref = await minecraft.getAttribute('href');
	const gameId = gameHref ? new URL(gameHref, state.baseURL).searchParams.get('game') : null;
	expect(gameId).toBeTruthy();
	await page.goto(`/definitions/games/${encodeURIComponent(gameId ?? '')}`);
	await expect(page.getByRole('heading', { name: 'Unnamed definition' })).toBeVisible();
	await expect(page.getByText('game', { exact: true })).toBeVisible();
	await page.getByText('Raw signed definition').click();
	await expect(page.getByRole('table')).toBeVisible();

	await page.goto('/search');
	const search = page.locator('#content').getByLabel('Search mods');
	await search.fill('does-not-exist');
	await search.press('Enter');
	await expect(page).toHaveURL(/q=does-not-exist/);
	await expect(page.getByText('No mods match')).toBeVisible();

	await page.setViewportSize({ width: 390, height: 844 });
	await page.goto('/');
	await page.locator('summary[aria-label="Menu"]').click();
	await expect(page.getByRole('link', { name: 'Browse', exact: true })).toBeVisible();
	await page.getByRole('link', { name: 'Instance information' }).click();
	await expect(page.getByRole('heading', { name: 'Instance information' })).toBeVisible();
});

test('registers, signs in, and deletes a public account', async ({ page }) => {
	const email = `browser-${Date.now()}@example.org`;
	const password = 'browser account password 123';

	await page.goto('/account');
	await page.getByRole('tab', { name: 'Create account' }).click();
	await page.getByLabel('Email').fill(email);
	await page.getByLabel('Password').fill(password);
	await page.getByRole('button', { name: 'Create account' }).click();
	await expect(page.getByRole('alert')).toContainText('Account created');

	await page.getByLabel('Email').fill(email);
	await page.getByLabel('Password').fill(password);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await expect(page.getByRole('button', { name: 'Sign out' })).toBeVisible();

	page.once('dialog', (dialog) => dialog.accept());
	await page.getByRole('button', { name: 'Delete my account' }).click();
	await expect(page.getByRole('button', { name: 'Sign in' })).toBeVisible();
});

test('covers operator account, dashboard, organizations, federation, and permissions', async ({
	page,
}) => {
	await signIn(page, state.password);
	const externalData = {
		source_class: 'external-catalog',
		canonical_source_url: 'https://modrinth.com/mod/e2e-project',
		observed_profile: { name: 'External E2E Project', summary: 'Observed by the bridge.' },
		files: [
			{
				external_file_id: 'release-1',
				source_url: 'https://cdn.example.org/e2e.jar',
				digest: '11'.repeat(32),
				size: 12,
				metadata: { filename: 'e2e.jar' },
				observed_at: Math.floor(Date.now() / 1000),
			},
		],
		observed_at: Math.floor(Date.now() / 1000),
		source_state: 'active',
		bridge_id: 'e2e-bridge',
		bridge_version: '1.0.0',
	};
	const externalProject = await page.evaluate(async (data) => {
		const csrf = document.cookie
			.split('; ')
			.find((entry) => entry.startsWith('moraine_csrf='))
			?.slice('moraine_csrf='.length);
		const response = await fetch('/v1/external-projects/modrinth/e2e-project', {
			method: 'PUT',
			headers: {
				'content-type': 'application/json',
				...(csrf ? { 'x-csrf-token': decodeURIComponent(csrf) } : {}),
			},
			body: JSON.stringify(data),
		});
		return { ok: response.ok, status: response.status, body: await response.text() };
	}, externalData);
	expect(externalProject.ok, `${externalProject.status} ${externalProject.body}`).toBeTruthy();
	await page.goto('/external/modrinth/e2e-project');
	await expect(page.getByRole('heading', { name: 'External E2E Project' })).toBeVisible();
	await page.getByLabel('Claimant reference').fill('author@example.org');
	await page.getByLabel('Challenge reference').fill('https://example.org/challenge/e2e');
	await page.getByRole('button', { name: 'Record claim' }).click();
	await expect(page.getByRole('alert')).toContainText('awaits review');

	await page.goto('/dashboard');
	await page.getByRole('tab', { name: 'Moderation' }).click();
	await expect(page.getByText('modrinth/e2e-project')).toBeVisible();
	await page.getByRole('button', { name: 'Approve' }).click();
	await expect(page.getByRole('alert')).toContainText('claim approved');
	await expect(page.getByText('No pending external project claims.')).toBeVisible();
	const rejectedExternalData = {
		...externalData,
		canonical_source_url: 'https://modrinth.com/mod/rejected-e2e',
		observed_profile: { name: 'Rejected External E2E Project', summary: 'Rejected by the test.' },
		files: [{ ...externalData.files[0], external_file_id: 'rejected-release-1' }],
	};
	const rejectedProject = await page.evaluate(async (data) => {
		const csrf = document.cookie
			.split('; ')
			.find((entry) => entry.startsWith('moraine_csrf='))
			?.slice('moraine_csrf='.length);
		const response = await fetch('/v1/external-projects/modrinth/rejected-e2e', {
			method: 'PUT',
			headers: {
				'content-type': 'application/json',
				...(csrf ? { 'x-csrf-token': decodeURIComponent(csrf) } : {}),
			},
			body: JSON.stringify(data),
		});
		return { ok: response.ok, status: response.status, body: await response.text() };
	}, rejectedExternalData);
	expect(rejectedProject.ok, `${rejectedProject.status} ${rejectedProject.body}`).toBeTruthy();
	await page.goto('/external/modrinth/rejected-e2e');
	await page.getByLabel('Claimant reference').fill('rejected@example.org');
	await page.getByLabel('Challenge reference').fill('https://example.org/challenge/rejected');
	await page.getByRole('button', { name: 'Record claim' }).click();
	await expect(page.getByRole('alert')).toContainText('awaits review');
	await page.goto('/dashboard');
	await page.getByRole('tab', { name: 'Moderation' }).click();
	await expect(page.getByText('modrinth/rejected-e2e')).toBeVisible();
	await page.getByRole('button', { name: 'Reject' }).click();
	await expect(page.getByRole('alert')).toContainText('claim rejected');
	await expect(page.getByText('No pending external project claims.')).toBeVisible();

	await page.goto('/account');
	await page.getByLabel('API key name').fill(`e2e-${Date.now()}`);
	await page.getByRole('button', { name: 'Create API key' }).click();
	await expect(page.getByLabel('New API key')).toBeVisible();
	await page.getByRole('button', { name: 'Revoke' }).first().click();
	await page.getByLabel('Webhook URL').fill('https://example.org/moraine-e2e');
	await page.getByRole('button', { name: 'Add webhook' }).click();
	await expect(page.getByText('https://example.org/moraine-e2e')).toBeVisible();
	await page.getByRole('button', { name: 'Revoke' }).click();
	await expect(page.getByText('https://example.org/moraine-e2e')).not.toBeVisible();

	await page.goto('/dashboard');
	await expect(page.getByRole('heading', { name: 'Dashboard' })).toBeVisible();

	await page.getByRole('tab', { name: 'Accounts' }).click();
	const memberEmail = `member-${Date.now()}@example.org`;
	await page.locator('main input[type="email"]').fill(memberEmail);
	await page.getByRole('button', { name: 'Create account' }).click();
	const accountNotice = page.getByRole('alert').filter({ hasText: memberEmail });
	await expect(accountNotice).toBeVisible();
	const accountText = (await accountNotice.textContent()) ?? '';
	let memberPassword = accountText.split('Temporary password: ')[1]?.split(' —')[0] ?? '';
	expect(memberPassword.length).toBeGreaterThan(12);
	const adminEmail = `admin-${Date.now()}@example.org`;
	await page.locator('main input[type="email"]').fill(adminEmail);
	await page.getByRole('button', { name: 'Create account' }).click();
	const adminNotice = page.getByRole('alert').filter({ hasText: adminEmail });
	await expect(adminNotice).toBeVisible();
	const adminPassword =
		((await adminNotice.textContent()) ?? '').split('Temporary password: ')[1]?.split(' —')[0] ??
		'';
	expect(adminPassword.length).toBeGreaterThan(12);
	const memberId = await page.evaluate(async (email) => {
		const response = await fetch('/v1/auth/users');
		const accounts = (await response.json()) as { user_id: string; email: string }[];
		return accounts.find((account) => account.email === email)?.user_id ?? '';
	}, memberEmail);
	expect(memberId).toMatch(/^[0-9a-f]{32}$/);

	const memberRow = page.getByText(memberEmail).locator('xpath=ancestor::li');
	await memberRow.getByRole('button', { name: 'reset' }).click();
	const resetNotice = page
		.getByRole('alert')
		.filter({ hasText: `Temporary password for ${memberEmail}` });
	await expect(resetNotice).toBeVisible();
	memberPassword = ((await resetNotice.textContent()) ?? '').split(': ').at(-1) ?? '';
	await memberRow.getByRole('button', { name: 'delete' }).isVisible();

	await page.getByRole('tab', { name: 'Definitions' }).click();
	await expect(page.getByRole('cell', { name: 'Minecraft' })).toBeVisible();
	const definitionLink = page
		.getByRole('cell', { name: 'Minecraft' })
		.locator('..')
		.getByRole('link');
	await definitionLink.click();
	await expect(page.getByRole('main')).toContainText('Minecraft');
	await page.goto('/dashboard');
	await page.getByRole('tab', { name: 'Definitions' }).click();
	await expect(page.getByText('No definition subscriptions yet.')).toBeVisible();
	await page.getByLabel('Definition ID').fill('gd:sha256:invalid');
	await page.getByLabel('From home').fill('http://127.0.0.1:1');
	await page.getByRole('button', { name: 'Subscribe and pull' }).click();
	await expect(page.getByRole('alert')).toBeVisible();
	const gameId = await page.evaluate(async () => {
		const response = await fetch('/v1/games');
		const games = (await response.json()) as { id: string }[];
		return games[0]?.id ?? '';
	});
	expect(gameId).toMatch(/^gd:sha256:/);
	await page.getByLabel('Definition ID').fill(gameId);
	await page.getByLabel('From home').fill(state.baseURL);
	await page.getByRole('button', { name: 'Subscribe and pull' }).click();
	await expect(page.getByRole('alert')).toContainText('Subscribed to game');
	await expect(page.getByText(`from ${state.baseURL}`)).toBeVisible();

	await page.goto('/dashboard');
	await page.getByRole('tab', { name: 'Federation' }).click();
	await expect(page.getByText('No homes followed')).toBeVisible();
	await page.getByRole('button', { name: 'Pull all now' }).click();
	await expect(page.getByRole('alert')).toContainText('pulled 0 project(s)');
	await page.getByLabel('Home URL').fill('http://127.0.0.1:1');
	await page.getByLabel('Project ID', { exact: true }).fill('gd:sha256:invalid');
	await page.getByRole('button', { name: 'Pull project' }).click();
	await expect(page.getByRole('alert')).toBeVisible();
	await page.getByLabel('Witness project ID').fill('gd:sha256:invalid');
	await page.getByRole('button', { name: 'Inspect witness log' }).click();
	await expect(page.getByText('No conflicting heads were observed.')).toBeVisible();
	await expect(page.getByText('No witness observations recorded.')).toBeVisible();
	await page.getByLabel('Mirror ID').fill('archive.example');
	await page.getByLabel('Mirror public key').fill('not-hex');
	await page.getByRole('button', { name: 'Pin mirror key' }).click();
	await expect(page.getByRole('alert')).toBeVisible();
	await page.getByLabel('Mirror ID').fill('archive.example');
	await page.getByLabel('Mirror public key').fill('11'.repeat(32));
	await page.getByRole('button', { name: 'Pin mirror key' }).click();
	await expect(page.getByRole('alert')).toContainText('Mirror key pinned');
	await page.getByRole('tab', { name: 'Policy' }).click();
	await expect(page.getByText('No local policy overrides')).toBeVisible();
	await page.getByLabel('Policy project id').fill('gd:sha256:policy-e2e');
	await page.getByLabel('Policy note').fill('e2e policy');
	await page.getByRole('button', { name: 'Set policy' }).click();
	await expect(page.getByText('gd:sha256:policy-e2e')).toBeVisible();
	const policyRow = page.getByText('gd:sha256:policy-e2e').locator('xpath=ancestor::li');
	await policyRow.getByRole('combobox', { name: 'State' }).selectOption('blocked');
	const blockedPolicyResponse = page.waitForResponse(
		(response) =>
			response.url().includes('/v1/directory/policy/') && response.request().method() === 'PUT',
	);
	await policyRow.getByRole('button', { name: 'Save' }).click();
	await expect((await blockedPolicyResponse).ok()).toBeTruthy();
	await expect(policyRow.getByRole('combobox', { name: 'State' })).toHaveValue('blocked');
	await policyRow.getByRole('combobox', { name: 'State' }).selectOption('listed');
	const listedPolicyResponse = page.waitForResponse(
		(response) =>
			response.url().includes('/v1/directory/policy/') && response.request().method() === 'PUT',
	);
	await policyRow.getByRole('button', { name: 'Save' }).click();
	await expect((await listedPolicyResponse).ok()).toBeTruthy();
	await expect(page.getByText('gd:sha256:policy-e2e')).toHaveCount(0);
	await page.getByLabel('Grant project id').fill('gd:sha256:invalid');
	await page.getByRole('button', { name: 'Load grants' }).click();
	await expect(page.getByText('No grants for this project.')).toBeVisible();
	await page.getByRole('tab', { name: 'Moderation' }).click();
	await expect(page.getByText('No sanctions recorded.')).toBeVisible();
	await expect(page.getByText('No impersonation reports recorded.')).toBeVisible();
	await page.getByLabel('Subject account id').fill(memberId);
	await page.getByLabel('Reason code').fill('policy-disallowed');
	await page.getByLabel('Scope id').fill(memberId);
	await page.getByRole('button', { name: 'Record sanction' }).click();
	await expect(
		page.getByText(new RegExp(`warning · ${memberId} · account:${memberId}`)),
	).toBeVisible();
	await page.getByLabel('Provider id').fill('scanner.example');
	await page.getByLabel('Provider public key').fill('not-hex');
	await page.getByRole('button', { name: 'Pin provider key' }).click();
	await expect(page.getByRole('alert')).toBeVisible();
	await page.getByLabel('Provider id').fill('scanner.example');
	await page.getByLabel('Provider public key').fill('22'.repeat(32));
	await page.getByRole('button', { name: 'Pin provider key' }).click();
	await expect(page.getByRole('alert')).toContainText('Provider key pinned');
	await page.getByLabel('Claim kind').fill('impersonation');
	await page.getByLabel('Report claimant').fill('claimant@example.org');
	await page.getByLabel('Reported handle').fill('reported-project');
	await page.getByLabel('Report evidence').fill('case-123');
	await page.getByRole('button', { name: 'Record impersonation report' }).click();
	await expect(page.getByText(/open · impersonation/)).toBeVisible();
	await page.getByLabel('Claimant reference').fill('rights@example.org');
	await page.getByLabel('Legal target id').fill('gd:sha256:invalid');
	await page.getByLabel('Stated basis').fill('documented request');
	await page.getByRole('button', { name: 'Record legal request' }).click();
	await expect(page.getByText(/Legal request recorded/)).toBeVisible();
	await page.getByRole('button', { name: 'Load recorded requests' }).click();
	await expect(page.getByText(/Loaded 1 legal request/)).toBeVisible();

	await page.goto('/orgs');
	const handle = `e2e-team-${Date.now()}`;
	await page.getByLabel('Organization handle').fill(handle);
	await page.getByLabel('Organization display name').fill('E2E Team');
	await page.getByRole('button', { name: 'Create' }).click();
	await expect(page.getByText('E2E Team')).toBeVisible();
	await page.getByText('E2E Team').click();
	await expect(page.getByRole('heading', { name: 'E2E Team' })).toBeVisible();
	await expect(page.getByText('No projects are owned by this organization yet.')).toBeVisible();

	await page.getByLabel('Team name').fill('Core');
	await page.getByRole('button', { name: 'Create team' }).click();
	await expect(page.getByText('Core')).toBeVisible();
	await page.getByLabel('Member email').fill(memberEmail);
	await page.getByRole('button', { name: 'Add member' }).click();
	await expect(page.getByText(memberEmail)).toBeVisible();
	await page.getByLabel('Member email').fill(adminEmail);
	await page.getByLabel('Role').click();
	await page.getByRole('option', { name: 'admin', exact: true }).click();
	await page.getByRole('button', { name: 'Add member' }).click();
	await expect(page.getByText(adminEmail)).toBeVisible();

	await page.goto('/account');
	const download = page.waitForEvent('download');
	await page.getByRole('button', { name: 'Export my data' }).click();
	await expect((await download).suggestedFilename()).toBe('moraine-account.json');
	await page.locator('summary[aria-label="Your account"]').click();
	await page.locator('details.dropdown').getByRole('button', { name: 'Sign out' }).click();

	await page.goto('/account');
	await page.getByLabel('Email').fill(memberEmail);
	await page.getByLabel('Password').fill(memberPassword);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await expect(page.getByRole('button', { name: 'Sign out' })).toBeVisible();

	await page.goto('/dashboard');
	await expect(page.getByText('You are not an operator')).toBeVisible();
	await page.goto('/notifications');
	await expect(page.getByText('No notifications')).toBeVisible();
	await page.goto('/projects');
	await expect(page.getByText('You are not following anything yet')).toBeVisible();
	await page.goto('/submissions');
	await expect(page.getByText('No submissions yet')).toBeVisible();
	await page.goto('/review');
	await expect(page.getByText('Nothing to review')).toBeVisible();

	await page.goto('/orgs');
	await page.getByText('E2E Team').click();
	await expect(page.getByText('Your role: member')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Add member' })).not.toBeVisible();

	await page.locator('summary[aria-label="Your account"]').click();
	await page.locator('details.dropdown').getByRole('button', { name: 'Sign out' }).click();

	await page.goto('/account');
	await page.getByLabel('Email').fill(adminEmail);
	await page.getByLabel('Password').fill(adminPassword);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await expect(page.getByRole('button', { name: 'Sign out' })).toBeVisible();
	await page.goto(`/orgs/${handle}`);
	await expect(page.getByText('Your role: admin')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Add member' })).toBeVisible();
	await page.locator('summary[aria-label="Your account"]').click();
	await page.locator('details.dropdown').getByRole('button', { name: 'Sign out' }).click();

	await signIn(page, state.password);
	await page.goto(`/orgs/${handle}`);
	const ownerAdminRow = page.getByText(adminEmail).locator('xpath=ancestor::li');
	await ownerAdminRow.getByRole('button', { name: `Remove ${adminEmail}` }).click();
	await expect(page.getByText(adminEmail)).not.toBeVisible();
	const ownerMemberRow = page.getByText(memberEmail).locator('xpath=ancestor::li');
	await ownerMemberRow.getByRole('button', { name: `Remove ${memberEmail}` }).click();
	await expect(page.getByText(memberEmail)).not.toBeVisible();

	await page.getByLabel('Team name').fill('Child');
	await page.getByRole('button', { name: 'Create team' }).click();
	await expect(page.getByText('Child')).toBeVisible();
	const childTeam = page.getByText('Child').locator('xpath=..');
	await childTeam.getByRole('button', { name: 'Move', exact: true }).click();
	await page.getByLabel('Move under').click();
	await page.getByRole('option', { name: 'Core' }).click();
	await childTeam.getByRole('button', { name: 'Move', exact: true }).click();
	await expect(page.getByText('Child')).toBeVisible();

	await page.locator('summary[aria-label="Your account"]').click();
	await page.locator('details.dropdown').getByRole('button', { name: 'Sign out' }).click();
	await page.goto('/account');
	await page.getByLabel('Email').fill(memberEmail);
	await page.getByLabel('Password').fill(memberPassword);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await expect(page.getByRole('button', { name: 'Sign out' })).toBeVisible();
	page.once('dialog', (dialog) => dialog.accept());
	await page.getByRole('button', { name: 'Delete my account' }).click();
	await expect(page.getByRole('button', { name: 'Sign in' })).toBeVisible();
	await page.getByLabel('Email').fill(memberEmail);
	await page.getByLabel('Password').fill(memberPassword);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await expect(page.getByRole('button', { name: 'Sign in' })).toBeVisible();

	const selfRegisteredEmail = `registered-${Date.now()}@example.org`;
	await page.getByRole('tab', { name: 'Create account' }).click();
	await page.getByLabel('Email').fill(selfRegisteredEmail);
	await page.getByLabel('Password').fill('registered password 123');
	await page.getByRole('button', { name: 'Create account' }).click();
	await expect(page.getByText('Account created. Sign in to continue.')).toBeVisible();
	await page.getByLabel('Email').fill(selfRegisteredEmail);
	await page.getByLabel('Password').fill('registered password 123');
	await page.getByRole('button', { name: 'Sign in' }).click();
	await expect(page.getByRole('heading', { name: selfRegisteredEmail })).toBeVisible();
	page.once('dialog', (dialog) => dialog.accept());
	await page.getByRole('button', { name: 'Delete my account' }).click();
	await expect(page.getByRole('button', { name: 'Sign in' })).toBeVisible();
	await page.getByLabel('Email').fill(selfRegisteredEmail);
	await page.getByLabel('Password').fill('registered password 123');
	await page.getByRole('button', { name: 'Sign in' }).click();
	await expect(page.getByRole('button', { name: 'Sign in' })).toBeVisible();
});
