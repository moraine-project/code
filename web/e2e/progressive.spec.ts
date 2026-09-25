import { readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';

const state = JSON.parse(readFileSync(join(tmpdir(), 'moraine-e2e.json'), 'utf8')) as {
	progressiveBaseURL: string;
	progressivePassword: string;
};

test('reviews a first release, auto-publishes the next, and revokes its grant', async ({
	page,
}) => {
	await page.goto(`${state.progressiveBaseURL}/account`);
	await page.getByLabel('Email').fill('ops@example.org');
	await page.getByLabel('Password').fill(state.progressivePassword);
	await page.locator('form').getByRole('button', { name: 'Sign in' }).click();
	await expect(page.getByRole('button', { name: 'Sign out' })).toBeVisible();

	await page.goto(`${state.progressiveBaseURL}/publish`);
	await page.getByRole('button', { name: 'Generate a key' }).click();
	await expect(page.getByText('Key ready')).toBeVisible();
	const seed = await page.getByRole('textbox', { name: 'Paste a key (hex)' }).inputValue();
	await page.getByRole('button', { name: 'Game', exact: true }).click();
	await page.getByRole('option', { name: 'Minecraft' }).click();
	await page.getByLabel('Name', { exact: true }).fill('Progressive Browser Test');
	await page.getByRole('button', { name: 'Create the project' }).click();
	const created = page.getByText(/Project created:/);
	await expect(created).toBeVisible({ timeout: 30_000 });
	const projectId = (await created.textContent())?.split(': ')[1]?.trim() ?? '';
	expect(projectId).toMatch(/^gd:sha256:/);
	await page.setInputFiles('input[aria-label="File to publish"]', {
		name: 'progressive-1.jar',
		mimeType: 'application/java-archive',
		buffer: Buffer.from('progressive first release'),
	});
	await page.getByRole('button', { name: 'Game versions' }).click();
	await page.getByRole('option', { name: '1.20.1' }).click();
	await page.keyboard.press('Escape');
	await page.getByLabel('Version', { exact: true }).fill('1.0.0');
	await page.getByRole('button', { name: 'Publish release' }).click();
	const firstRelease = page.getByText(/Release signed:/);
	await expect(firstRelease).toBeVisible({ timeout: 30_000 });
	const releaseId = (await firstRelease.textContent())?.split(': ').at(-1)?.trim() ?? '';
	expect(releaseId).toMatch(/^gd:sha256:/);
	await expect(page.getByText(/Submitted for review/).last()).toBeVisible();

	await page.goto(`${state.progressiveBaseURL}/review`);
	const row = page
		.locator('tr')
		.filter({ hasText: releaseId.slice('gd:sha256:'.length, 'gd:sha256:'.length + 16) });
	await row.getByRole('button', { name: 'Take' }).click();
	await row.getByRole('button', { name: 'Accept' }).click();
	await expect(page.getByRole('alert')).toContainText('accept recorded');

	await page.goto(`${state.progressiveBaseURL}/dashboard`);
	await page.getByRole('button', { name: 'Policy' }).click();
	await page.getByLabel('Grant project id').fill(projectId);
	await page.getByRole('button', { name: 'Load grants' }).click();
	await expect(page.getByText(/user:.*·.*active/)).toBeVisible();

	await page.goto(`${state.progressiveBaseURL}/publish`);
	await page.getByRole('button', { name: 'Advanced' }).click();
	await page.getByRole('textbox', { name: 'Paste a key (hex)' }).fill(seed);
	await expect(page.getByText('Key ready')).toBeVisible();
	await page.getByRole('button', { name: 'Game', exact: true }).click();
	await page.getByRole('option', { name: 'Minecraft' }).click();
	await page.getByRole('button', { name: 'Existing project' }).click();
	await page.getByLabel('Project ID').fill(projectId);
	await page.setInputFiles('input[aria-label="File to publish"]', {
		name: 'progressive-2.jar',
		mimeType: 'application/java-archive',
		buffer: Buffer.from('progressive second release'),
	});
	await page.getByRole('button', { name: 'Game versions' }).click();
	await page.getByRole('option', { name: '1.20.1' }).click();
	await page.keyboard.press('Escape');
	await page.getByLabel('Version', { exact: true }).fill('1.0.1');
	await page.getByRole('button', { name: 'Publish release' }).click();
	await expect(page.getByText(/Published automatically:/)).toBeVisible({ timeout: 30_000 });

	await page.goto(`${state.progressiveBaseURL}/dashboard`);
	await page.getByRole('button', { name: 'Policy' }).click();
	await page.getByLabel('Grant project id').fill(projectId);
	await page.getByRole('button', { name: 'Load grants' }).click();
	await expect(page.getByText(/user:.*·.*active/)).toBeVisible();
	await page.getByRole('button', { name: 'Revoke active grants' }).click();
	await expect(page.getByText(/user:.*·.*revoked/)).toBeVisible();
});
