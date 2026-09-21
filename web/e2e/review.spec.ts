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

const reviewUrl = (path: string) => `${state.reviewBaseURL}${path}`;
const short = (id: string) => id.slice('gd:sha256:'.length, 'gd:sha256:'.length + 16);

test('submits, assigns, and accepts a release in review mode', async ({ page }) => {
	await page.goto(reviewUrl('/account'));
	await page.getByLabel('Email').fill('ops@example.org');
	await page.getByLabel('Password').fill(state.reviewPassword);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await expect(page.getByRole('button', { name: 'Sign out' })).toBeVisible();

	await page.goto(reviewUrl('/publish'));
	await page.getByRole('button', { name: 'Generate a key' }).click();
	await expect(page.getByText('Key ready')).toBeVisible();
	await page.getByRole('button', { name: 'Game', exact: true }).click();
	await page.getByRole('option', { name: 'Minecraft' }).click();
	await page.getByRole('button', { name: 'Create the project' }).click();
	const created = page.getByText(/Project created:/);
	await expect(created).toBeVisible({ timeout: 30_000 });
	const projectId = (await created.textContent())?.split(': ')[1]?.trim() ?? '';
	expect(projectId).toMatch(/^gd:sha256:/);
	await page.setInputFiles('input[aria-label="File to publish"]', {
		name: 'review-example.jar',
		mimeType: 'application/java-archive',
		buffer: Buffer.from('review mod bytes'),
	});
	await page.getByRole('button', { name: 'Game versions' }).click();
	await page.getByRole('option', { name: '1.20.1' }).click();
	await page.keyboard.press('Escape');
	await page.getByLabel('Version', { exact: true }).fill('1.0.0');
	await page.getByRole('button', { name: 'Publish release' }).click();
	const releaseLine = page.getByText(/Release signed:/);
	await expect(releaseLine).toBeVisible({ timeout: 30_000 });
	const releaseId = (await releaseLine.textContent())?.split(': ').at(-1)?.trim() ?? '';
	expect(releaseId).toMatch(/^gd:sha256:/);
	await expect(page.getByText(/Submitted for review/).last()).toBeVisible();

	await page.goto(reviewUrl('/review'));
	const releaseRow = page.locator('tr').filter({ hasText: short(releaseId) });
	await releaseRow.getByRole('button', { name: 'Take' }).click();
	await expect(page.getByRole('alert')).toContainText('assigned');
	await releaseRow.getByRole('button', { name: 'Accept' }).click();
	await expect(page.getByRole('alert')).toContainText('accept recorded');
	await expect(page.getByText('Nothing to review')).toBeVisible();

	await page.goto(reviewUrl('/submissions'));
	await expect(page.getByText('accepted', { exact: true })).toBeVisible();

	await page.goto(`${state.baseURL}/account`);
	await page.getByLabel('Email').fill('ops@example.org');
	await page.getByLabel('Password').fill(state.password);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await expect(page.getByRole('button', { name: 'Sign out' })).toBeVisible();
	await page.goto(`${state.baseURL}/dashboard`);
	await page.getByRole('tab', { name: 'Federation' }).click();
	await page.getByLabel('Home URL').fill(state.reviewBaseURL);
	await page.getByLabel('Project ID', { exact: true }).fill(projectId);
	await page.getByRole('button', { name: 'Pull project' }).click();
	const subscription = page.locator('tr').filter({
		hasText: projectId.replace(/^gd:sha256:/, '').slice(0, 12),
	});
	await expect(subscription).toBeVisible();
	await subscription.getByRole('button', { name: 'reset' }).click();
	await expect(page.getByRole('alert')).toContainText('cursor reset');
	await subscription.getByRole('button', { name: 'unfollow' }).click();
	await expect(page.getByText('No homes followed')).toBeVisible();
});
