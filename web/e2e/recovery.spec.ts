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

test('changes a password and recovers with a one-time code', async ({ page }) => {
	await signIn(page, state.password);

	await page.getByRole('button', { name: 'Generate new codes' }).click();
	await expect(page.getByText(/shown only once/)).toBeVisible();
	const code = (await page.locator('ul.font-mono li').first().textContent())?.trim() ?? '';
	expect(code.length).toBeGreaterThan(20);

	await page.getByLabel('Current password').fill(state.password);
	await page.getByLabel('New password').fill('a changed password 123');
	await page.getByRole('button', { name: 'Change password' }).click();
	await expect(page.getByText(/Password changed/)).toBeVisible();

	await page.getByRole('button', { name: 'Sign out' }).click();
	await expect(page.getByRole('button', { name: 'Sign in' })).toBeVisible();

	await page.getByRole('button', { name: 'Forgot your password?' }).click();
	await page.getByLabel('Email').fill('ops@example.org');
	await page.getByLabel('Recovery code').fill(code);
	await page.getByLabel('New password').fill('recovered password 456');
	await page.getByRole('button', { name: 'Set a new password' }).click();
	await expect(page.getByText(/Password changed. Sign in/)).toBeVisible();

	await signIn(page, 'recovered password 456');
});
