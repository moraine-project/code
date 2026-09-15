import { readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';

const state = JSON.parse(readFileSync(join(tmpdir(), 'moraine-e2e.json'), 'utf8')) as {
	baseURL: string;
	password: string;
};

test('signs in, publishes a release in the browser, and finds it', async ({ page }) => {
	await page.goto('/');
	await expect(page.getByRole('heading', { name: /Mods for the games you play/i })).toBeVisible();

	await page.goto('/account');
	await page.getByLabel('Email').fill('ops@example.org');
	await page.getByLabel('Password').fill(state.password);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await expect(page.getByRole('button', { name: 'Sign out' })).toBeVisible();

	await page.goto('/publish');
	await page.getByRole('button', { name: 'Generate a key' }).click();
	await expect(page.getByText('Key ready')).toBeVisible();

	await page.getByRole('button', { name: 'Game', exact: true }).click();
	await page.getByRole('option', { name: 'Minecraft' }).click();
	await page.getByLabel('Name', { exact: true }).fill('Browser Test Mod');
	await page.getByRole('button', { name: 'Create the project' }).click();
	const created = page.getByText(/Project created:/);
	await expect(created).toBeVisible({ timeout: 30_000 });
	const projectId = (await created.textContent())?.split(': ')[1]?.trim() ?? '';
	expect(projectId).toContain('gd:sha256:');

	await page.setInputFiles('input[aria-label="File to publish"]', {
		name: 'example.jar',
		mimeType: 'application/java-archive',
		buffer: Buffer.from('mod bytes'),
	});
	await page.getByRole('button', { name: 'Game versions' }).click();
	await page.getByRole('option', { name: '1.20.1' }).click();
	await page.keyboard.press('Escape');
	await page.getByLabel('Version', { exact: true }).fill('1.0.0');
	await page.getByRole('button', { name: 'Publish release' }).click();
	await expect(page.getByText(/Release signed:/)).toBeVisible({ timeout: 30_000 });
	await expect(page.getByText(/Feed entry 2:/)).toBeVisible();

	await page.goto(`/p/${encodeURIComponent(projectId)}`);
	await expect(page.getByRole('heading', { name: 'Browser Test Mod' })).toBeVisible();
	await expect(page.getByRole('link', { name: 'Download' })).toBeVisible();

	await page.goto('/search?q=' + encodeURIComponent('Browser Test Mod'));
	await expect(page.getByText('Browser Test Mod')).toBeVisible();
});
