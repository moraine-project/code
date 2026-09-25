import { readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';

const state = JSON.parse(readFileSync(join(tmpdir(), 'moraine-e2e.json'), 'utf8')) as {
	baseURL: string;
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
	await page.locator('form').getByRole('button', { name: 'Sign in' }).click();
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
	await page.locator('form').getByRole('button', { name: 'Sign in' }).click();
	await page.goto(`${state.reviewBaseURL}/review`);
	await expect(page.getByRole('button', { name: 'Load more' })).toBeVisible();
	await page.getByRole('button', { name: 'Load more' }).click();
	await expect.poll(() => cursor).toBe('901:submission-99');
	await expect(page.locator('tbody tr')).toHaveCount(101);
	await expect(page.locator('code[title="gd:sha256:' + '4'.repeat(64) + '"]')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Load more' })).not.toBeVisible();
});

test('pages back through a project feed that is longer than one page', async ({ page }) => {
	const projectId = 'gd:sha256:' + '1'.repeat(64);
	const headSeq = 60;
	const requested: number[] = [];

	page.route(
		(url) => url.pathname.startsWith(`/v1/projects/${encodeURIComponent(projectId)}`),
		async (route) => {
			const path = new URL(route.request().url()).pathname;
			if (path.endsWith('/feed')) {
				const after = Number(new URL(route.request().url()).searchParams.get('after') ?? 0);
				requested.push(after);
				const limit = 50;
				const entries = [];
				for (let seq = after + 1; seq <= headSeq && entries.length < limit; seq += 1) {
					entries.push({
						seq,
						kind: 'release-published',
						title: `Release ${seq}`,
						release: { channel: 'release', game_id: '', loaders: [] },
						object: `gd:sha256:${String(seq).padStart(64, '0')}`,
						entry: `gd:sha256:${String(seq).padStart(64, '0')}`,
						declared_at: 1_700_000_000 + seq,
					});
				}
				await route.fulfill({
					contentType: 'application/json',
					body: JSON.stringify({ project_id: projectId, head_seq: headSeq, entries }),
				});
				return;
			}
			if (path === `/v1/projects/${encodeURIComponent(projectId)}`) {
				await route.fulfill({
					contentType: 'application/json',
					body: JSON.stringify({
						project_id: projectId,
						genesis: 'gd:sha256:' + '2'.repeat(64),
						head_seq: headSeq,
						listing_state: 'listed',
					}),
				});
				return;
			}
			await route.fallback();
		},
	);

	await page.goto(`/p/${encodeURIComponent(projectId)}`);
	await page.getByRole('button', { name: /Versions/ }).click();
	await expect(page.getByRole('link', { name: 'View' })).toHaveCount(50);
	await expect(page.getByRole('link', { name: 'View' }).first()).toHaveAttribute(
		'href',
		new RegExp(`release/0*${headSeq}$`),
	);
	await expect(page.getByRole('button', { name: 'Load older releases' })).toBeVisible();

	await page.getByRole('button', { name: 'Load older releases' }).click();
	await expect(page.getByRole('link', { name: 'View' })).toHaveCount(headSeq);
	await expect(page.getByRole('link', { name: 'View' }).first()).toHaveAttribute(
		'href',
		new RegExp(`release/0*${headSeq}$`),
	);
	await expect(page.getByRole('link', { name: 'View' }).last()).toHaveAttribute(
		'href',
		/release\/0*1$/,
	);
	await expect(page.getByRole('button', { name: 'Load older releases' })).not.toBeVisible();
	expect(requested).toEqual([10, 0]);
});

test('reports the search total beside the page it loaded', async ({ page }) => {
	page.route(
		(url) => url.pathname === '/v1/search',
		async (route) => {
			const url = new URL(route.request().url());
			const cursor = url.searchParams.get('cursor');
			await route.fulfill({
				contentType: 'application/json',
				body: JSON.stringify({
					protocol: 1,
					results: [
						{
							project_id: 'gd:sha256:' + '3'.repeat(64),
							game_id: 'gd:sha256:' + '4'.repeat(64),
							display_name: cursor ? 'Second page mod' : 'First page mod',
							summary: '',
							listing_state: 'listed',
							source_instance: state.baseURL,
						},
					],
					next_cursor: cursor ? null : 'page-2',
					total_estimate: 42,
				}),
			});
		},
	);

	await page.goto('/search?q=mod');
	await expect(page.getByText('1 on this page · about 42 in total')).toBeVisible();
	await page.getByRole('link', { name: 'Next page' }).click();
	await expect(page.getByText('Second page mod')).toBeVisible();
	await expect(page.getByText('1 on this page · about 42 in total')).toBeVisible();
	await expect(page.getByRole('link', { name: 'First page' })).toBeVisible();
});
