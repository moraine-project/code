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

test('a foreign home refuses the publishing console', async ({ page }) => {
	await page.goto(`/publish?home=${encodeURIComponent(foreign)}`);
	await expect(page.getByText('Publishing stays on your own home')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Generate a key' })).toHaveCount(0);
	await expect(page.getByRole('button', { name: 'Publish release' })).toHaveCount(0);
});

test('a sort-only search is treated as a search, not as an empty form', async ({ page }) => {
	for (const sort of ['updated', 'popularity']) {
		await page.goto(`/search?sort=${sort}`);
		await expect(page.getByText('Start with a search or a filter')).toHaveCount(0);
		await expect(page.locator('#content')).toContainText(
			/No mods match|Showing \d+|^\d+ results?$/,
		);
	}
});

test('a cursor in the URL reaches the search request and the first page drops it', async ({
	page,
}) => {
	const requested: string[] = [];
	page.on('request', (request) => {
		const url = new URL(request.url());
		if (url.pathname === '/v1/search') requested.push(url.search);
	});

	await page.goto('/search?q=e2e&cursor=20%3Ax');
	await expect(page.getByRole('link', { name: 'First page' })).toBeVisible();
	expect(requested.some((search) => search.includes('cursor=20%3Ax'))).toBe(true);

	await page.getByRole('link', { name: 'First page' }).click();
	await expect(page).not.toHaveURL(/cursor=/);
	expect(requested.at(-1)).not.toContain('cursor=');
});
