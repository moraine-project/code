import { expect, test, type Page } from '@playwright/test';

const scheme = (page: Page) => page.evaluate(() => document.documentElement.dataset.theme);

const expectScheme = (page: Page, expected: string) =>
	expect.poll(() => scheme(page)).toBe(expected);

const primary = (page: Page) =>
	page.evaluate(() =>
		getComputedStyle(document.documentElement).getPropertyValue('--color-primary').trim(),
	);

test.describe.configure({ mode: 'serial' });

test('the shipped dark theme applies without an operating-system hint', async ({ page }) => {
	await page.emulateMedia({ colorScheme: 'dark' });
	await page.goto('/');
	await expect(page.locator('main')).toBeVisible();
	await expectScheme(page, 'moraine');
});

test('the light scheme survives a reload', async ({ page }) => {
	await page.goto('/');
	await expectScheme(page, 'moraine');

	await page.getByRole('button', { name: 'Switch to the light theme' }).click();
	await expectScheme(page, 'moraine-light');

	await page.reload();
	await expect(page.locator('main')).toBeVisible();
	await expectScheme(page, 'moraine-light');

	await page.getByRole('button', { name: 'Switch to the dark theme' }).click();
	await expectScheme(page, 'moraine');
	await page.reload();
	await expect(page.locator('main')).toBeVisible();
	await expectScheme(page, 'moraine');
});

test('the two schemes really do resolve to different palettes', async ({ page }) => {
	await page.goto('/');
	await expectScheme(page, 'moraine');
	const dark = await primary(page);

	await page.getByRole('button', { name: 'Switch to the light theme' }).click();
	await expectScheme(page, 'moraine-light');
	const light = await primary(page);

	expect(light).not.toBe('');
	expect(light).not.toBe(dark);
});

test('an unknown stored preference falls back to the default', async ({ page }) => {
	await page.goto('/');
	await expectScheme(page, 'moraine');
	await page.evaluate(() => localStorage.setItem('moraine-color-scheme', 'neon'));
	await page.reload();
	await expect(page.locator('main')).toBeVisible();
	await expectScheme(page, 'moraine');
});
