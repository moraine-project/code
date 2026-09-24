import { expect, test } from '@playwright/test';

const foreign = 'http://127.0.0.1:8139';

test.describe.configure({ mode: 'serial' });

test('a foreign home is announced and offers a way back', async ({ page }) => {
	await page.goto(`/search?home=${encodeURIComponent(foreign)}`);
	await expect(page.getByText('Read only')).toBeVisible();
	await expect(page.getByText(foreign)).toBeVisible();
	await expect(page.getByRole('link', { name: /^Back to/ })).toBeVisible();
});

test('a foreign home keeps the home parameter on navigation', async ({ page }) => {
	await page.goto(`/search?home=${encodeURIComponent(foreign)}`);
	await expect(page.getByText('Read only')).toBeVisible();

	await page.getByRole('link', { name: 'Browse', exact: true }).first().click();
	await expect(page).toHaveURL(new RegExp(`home=${encodeURIComponent(foreign)}`));
	await expect(page.getByText('Read only')).toBeVisible();
});

test('the local home shows no read-only banner', async ({ page }) => {
	await page.goto('/search');
	await expect(page.getByRole('link', { name: 'Browse', exact: true }).first()).toBeVisible();
	await expect(page.getByText('Read only')).toHaveCount(0);
});

test('the document title carries the active home name', async ({ page }) => {
	await page.goto('/search');
	await expect(page).toHaveTitle('Mods · Moraine');
});
