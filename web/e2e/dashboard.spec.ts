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
	await page.locator('form').getByRole('button', { name: 'Sign in' }).click();
	await expect(page.getByRole('button', { name: 'Sign out' })).toBeVisible();

	await page.goto('/dashboard');
	await expect(page.getByRole('heading', { name: 'Dashboard' })).toBeVisible();
	await expect(page.getByText('Projects', { exact: true })).toBeVisible();

	await page.getByRole('button', { name: 'Accounts' }).click();
	await expect(page.getByRole('main').getByText('ops@example.org')).toBeVisible();

	await page.getByRole('button', { name: 'Definitions' }).click();
	await expect(page.getByRole('cell', { name: 'Minecraft' })).toBeVisible();

	await page.getByRole('button', { name: 'Records' }).click();
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
	await page.route('**/v1/scanners', async (route) => {
		if (route.request().method() === 'GET') {
			await route.fulfill({
				contentType: 'application/json',
				body: JSON.stringify([
					{
						provider_id: 'local-clamav',
						kind: 'clamav',
						command: 'clamscan',
						args: [],
						public_key: '00',
						enabled: true,
					},
				]),
			});
		} else {
			await route.fulfill({
				contentType: 'application/json',
				body: JSON.stringify({ provider_id: 'local-clamav' }),
			});
		}
	});
	await page.route('**/v1/scans', async (route) => {
		if (route.request().method() === 'GET') {
			await route.fulfill({
				contentType: 'application/json',
				body: JSON.stringify([
					{
						id: 'scan-1',
						provider_id: 'local-clamav',
						artifact_digest: 'sha256:test',
						status: 'succeeded',
						requested_by: 'ops',
						attempts: 1,
						result: {
							provider: 'local-clamav',
							kind: 'clamav',
							verdict: 'finding',
							findings: [{ message: 'test signature FOUND' }],
							exit_code: 1,
							raw: { stdout: 'test signature FOUND', stderr: '' },
						},
					},
				]),
			});
		} else {
			await route.fulfill({
				contentType: 'application/json',
				body: JSON.stringify({ id: 'scan-2' }),
			});
		}
	});
	await page.route('**/v1/scans/*/rescan', async (route) => {
		await route.fulfill({
			contentType: 'application/json',
			body: JSON.stringify({ id: 'scan-3' }),
		});
	});
	await page.route('**/v1/scanner-policies', async (route) => {
		await route.fulfill({
			contentType: 'application/json',
			body: JSON.stringify(route.request().method() === 'GET' ? [] : {}),
		});
	});
	await page.route('**/v1/scanner-subscriptions', async (route) => {
		await route.fulfill({
			contentType: 'application/json',
			body: JSON.stringify(route.request().method() === 'GET' ? [] : { id: 'subscription-1' }),
		});
	});
	await page.getByRole('button', { name: 'Scanners' }).click();
	await expect(page.getByRole('heading', { name: 'Scanner provider' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Manual scan' })).toBeVisible();
	await page.getByRole('heading', { name: 'Scan jobs' }).scrollIntoViewIfNeeded();
	await expect(page.getByRole('cell', { name: 'succeeded' })).toBeVisible();
	await expect(page.getByRole('cell', { name: 'finding' })).toBeVisible();
	await expect(page.getByText('test signature FOUND', { exact: true })).toBeVisible();
	await expect(page.getByText('Raw scanner evidence')).toBeVisible();
});
