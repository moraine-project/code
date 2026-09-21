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
	await page.getByRole('button', { name: 'Clear' }).click();
	await expect(page).toHaveURL(/\/search$/);

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

test('covers operator account, dashboard, organizations, federation, and permissions', async ({
	page,
}) => {
	await signIn(page, state.password);
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
	await page.getByRole('tab', { name: 'Federation' }).click();
	await expect(page.getByText('No homes followed')).toBeVisible();
	await page.getByRole('button', { name: 'Pull all now' }).click();
	await expect(page.getByRole('alert')).toContainText('pulled 0 project(s)');
	await page.getByLabel('Home URL').fill('http://127.0.0.1:1');
	await page.getByLabel('Project ID').fill('gd:sha256:invalid');
	await page.getByRole('button', { name: 'Pull project' }).click();
	await expect(page.getByRole('alert')).toBeVisible();
	await page.getByRole('tab', { name: 'Policy' }).click();
	await expect(page.getByText('No local policy overrides')).toBeVisible();
	await page.getByLabel('Grant project id').fill('gd:sha256:invalid');
	await page.getByRole('button', { name: 'Load grants' }).click();
	await expect(page.getByText('No grants for this project.')).toBeVisible();
	await page.getByRole('tab', { name: 'Moderation' }).click();
	await expect(page.getByText('No sanctions recorded.')).toBeVisible();
	await expect(page.getByText('No impersonation reports recorded.')).toBeVisible();
	await page.getByLabel('Provider id').fill('scanner.example');
	await page.getByLabel('Provider public key').fill('not-hex');
	await page.getByRole('button', { name: 'Pin provider key' }).click();
	await expect(page.getByRole('alert')).toBeVisible();
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

	await page.goto('/account');
	const download = page.waitForEvent('download');
	await page.getByRole('button', { name: 'Export my data' }).click();
	await expect((await download).suggestedFilename()).toBe('moraine-account.json');
	await page.getByRole('button', { name: 'Sign out' }).click();

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
});
