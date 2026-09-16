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
});
