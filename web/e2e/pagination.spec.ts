import { readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';

const state = JSON.parse(readFileSync(join(tmpdir(), 'moraine-e2e.json'), 'utf8')) as {
	password: string;
	reviewBaseURL: string;
	reviewPassword: string;
};

function submission(index: number) {
	return {
		submission: {
			id: `submission-${index}`,
			project_id: 'gd:sha256:' + '1'.repeat(64),
			object: 'gd:sha256:' + String(index).padStart(64, '0'),
			entry: 'gd:sha256:' + '2'.repeat(64),
			state: 'accepted',
			assigned_to: null,
			submitted_by: 'user-e2e',
			created_at: 1000 - index,
			updated_at: 1000 - index,
		},
		decisions: [],
	};
}

test('loads older submissions with the cursor from the last visible item', async ({ page }) => {
	let cursor: string | null = null;
	await page.route('**/v1/submissions**', async (route) => {
		const url = new URL(route.request().url());
		cursor = url.searchParams.get('cursor');
		const body = cursor
			? [
					{
						...submission(50),
						submission: { ...submission(50).submission, object: 'gd:sha256:' + '3'.repeat(64) },
					},
				]
			: Array.from({ length: 50 }, (_, index) => submission(index));
		await route.fulfill({ contentType: 'application/json', body: JSON.stringify(body) });
	});

	await page.goto('/account');
	await page.getByLabel('Email').fill('ops@example.org');
	await page.getByLabel('Password').fill(state.password);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await page.goto('/submissions');
	await expect(page.getByRole('button', { name: 'Load older submissions' })).toBeVisible();
	await page.getByRole('button', { name: 'Load older submissions' }).click();
	await expect(page.locator('code[title="gd:sha256:' + '3'.repeat(64) + '"]')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Load older submissions' })).not.toBeVisible();
	expect(cursor).toBe('951:submission-49');
});

test('loads more review items with the review queue cursor', async ({ page }) => {
	let cursor: string | null = null;
	await page.route('**/v1/review-queue**', async (route) => {
		const url = new URL(route.request().url());
		cursor = url.searchParams.get('cursor');
		const body = cursor
			? [{ ...submission(100).submission, object: 'gd:sha256:' + '4'.repeat(64) }]
			: Array.from({ length: 100 }, (_, index) => submission(index).submission);
		await route.fulfill({ contentType: 'application/json', body: JSON.stringify(body) });
	});

	await page.goto(`${state.reviewBaseURL}/account`);
	await page.getByLabel('Email').fill('ops@example.org');
	await page.getByLabel('Password').fill(state.reviewPassword);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await page.goto(`${state.reviewBaseURL}/review`);
	await expect(page.getByRole('button', { name: 'Load more' })).toBeVisible();
	await page.getByRole('button', { name: 'Load more' }).click();
	await expect.poll(() => cursor).toBe('901:submission-99');
	await expect(page.locator('tbody tr')).toHaveCount(101);
	await expect(page.locator('code[title="gd:sha256:' + '4'.repeat(64) + '"]')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Load more' })).not.toBeVisible();
});
