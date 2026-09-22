import { readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';

const state = JSON.parse(readFileSync(join(tmpdir(), 'moraine-e2e.json'), 'utf8')) as {
	baseURL: string;
	password: string;
};

test('an operator sees the dashboard and its accounts', async ({ page }) => {
	await page.goto('/account');
	await page.getByLabel('Email').fill('ops@example.org');
	await page.getByLabel('Password').fill(state.password);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await expect(page.getByRole('button', { name: 'Sign out' })).toBeVisible();

	await page.goto('/dashboard');
	await expect(page.getByRole('heading', { name: 'Dashboard' })).toBeVisible();
	await expect(page.getByText('Projects', { exact: true })).toBeVisible();

	await page.getByRole('tab', { name: 'Accounts' }).click();
	await expect(page.getByRole('main').getByText('ops@example.org')).toBeVisible();

	await page.getByRole('tab', { name: 'Definitions' }).click();
	await expect(page.getByRole('cell', { name: 'Minecraft' })).toBeVisible();

	await page.getByRole('tab', { name: 'Records' }).click();
	await expect(page.getByRole('heading', { name: 'Signed advisory' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Artifact locations' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'External catalog observation' })).toBeVisible();
	for (const path of ['advisories', 'attestations', 'deny-lists', 'mirror-commitments']) {
		await page.route(`**/v1/${path}`, async (route) => {
			await route.fulfill({
				contentType: 'application/json',
				body: JSON.stringify({ accepted: path }),
			});
		});
	}
	await page.route('**/v1/external-projects/modrinth/example-mod', async (route) => {
		await route.fulfill({
			contentType: 'application/json',
			body: JSON.stringify({
				provider: 'modrinth',
				external_project_id: 'example-mod',
				source_class: 'external-catalog',
				canonical_source_url: 'https://example.org/project',
				observed_profile: { name: 'Example project' },
				files: [],
				observed_at: 1,
				last_synced_at: 1,
				source_state: 'active',
				bridge_id: 'operator-bridge',
				bridge_version: '1',
			}),
		});
	});
	await page.getByLabel('Signed advisory signed wire').fill('00');
	await page.getByRole('button', { name: 'Publish signed advisory' }).click();
	await expect(page.getByRole('alert')).toContainText('advisory accepted');
	for (const [title, action] of [
		['Signed attestation', 'attestation'],
		['Signed deny list', 'deny-list'],
		['Mirror commitment', 'mirror-commitment'],
	] as const) {
		await page.getByLabel(`${title} signed wire`).fill('00');
		await page.getByRole('button', { name: `Publish ${title.toLowerCase()}` }).click();
		await expect(page.getByRole('alert')).toContainText(`${action} accepted`);
	}
	await page.getByLabel('External provider').fill('modrinth');
	await page.getByLabel('External project ID').fill('example-mod');
	await page.getByLabel('Observation JSON').fill(
		JSON.stringify({
			source_class: 'external-catalog',
			canonical_source_url: 'https://example.org/project',
			observed_profile: { name: 'Example project' },
			files: [],
			observed_at: 1,
			last_synced_at: 1,
			source_state: 'active',
			bridge_id: 'operator-bridge',
			bridge_version: '1',
		}),
	);
	await page.getByRole('button', { name: 'Save observation' }).click();
	await expect(page.getByRole('alert')).toContainText('External catalog observation saved');
	await page.getByRole('button', { name: 'Load observation' }).click();
	await expect(page.getByRole('alert')).toContainText('Loaded external catalog observation');
});
