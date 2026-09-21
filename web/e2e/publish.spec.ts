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
	const gameId = await page.evaluate(async () => {
		const response = await fetch('/v1/games');
		const games = (await response.json()) as { id: string }[];
		return games[0]?.id ?? '';
	});
	expect(gameId).toMatch(/^gd:sha256:/);
	await page.getByLabel('Name', { exact: true }).fill('Browser Test Mod');
	await page.getByRole('button', { name: 'Create the project' }).click();
	const created = page.getByText(/Project created:/);
	await expect(created).toBeVisible({ timeout: 30_000 });
	const projectId = (await created.textContent())?.split(': ')[1]?.trim() ?? '';
	expect(projectId).toContain('gd:sha256:');
	await page.getByRole('tab', { name: 'Existing project' }).click();
	await expect(page.getByRole('button', { name: 'Publish the profile' })).toBeVisible();
	await page.getByRole('button', { name: 'Publish the profile' }).click();
	await expect(page.getByText(/Profile published:/)).toBeVisible({ timeout: 30_000 });
	await page.getByLabel('Artifact kind').selectOption('modpack');
	await expect(page.getByLabel('Signed modpack manifest JSON')).toBeVisible();
	await page.getByLabel('Artifact kind').selectOption('mod');
	const followPage = await page.context().newPage();
	await followPage.goto(`/p/${encodeURIComponent(projectId)}`);
	await expect(followPage.getByRole('heading', { name: 'Browser Test Mod' })).toBeVisible();
	await followPage.getByRole('button', { name: 'Follow' }).click();
	await expect(followPage.getByRole('button', { name: 'Unfollow' })).toBeVisible();
	await followPage.goto('/projects');
	await expect(followPage.getByRole('heading', { name: 'Following' })).toBeVisible();
	await expect(followPage.getByText('Browser Test Mod')).toBeVisible();
	await followPage.close();

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
	const releaseId =
		(await page.getByText(/Release signed:/).textContent())?.split(': ').at(-1)?.trim() ?? '';
	expect(releaseId).toMatch(/^gd:sha256:/);
	await page.getByLabel('Release object id').fill(releaseId);
	await page.getByLabel('Withdrawal reason').selectOption('broken');
	await page.getByLabel('Withdrawal note').fill('e2e withdrawal');
	await page.getByRole('button', { name: 'Publish withdrawal' }).click();
	await expect(page.getByText(/Withdrawal published:/)).toBeVisible({ timeout: 30_000 });

	await page.setInputFiles('input[aria-label="File to publish"]', {
		name: 'example-pack.mrpack',
		mimeType: 'application/zip',
		buffer: Buffer.from('pack bytes'),
	});
	await page.getByLabel('Artifact kind').selectOption('modpack');
	await page.getByLabel('Version', { exact: true }).fill('2.0.0');
	await page.getByLabel('Signed modpack manifest JSON').fill(
		JSON.stringify({
			protocol: 1,
			project_id: projectId,
			game_id: gameId,
			entries: [
				{
					ordinal: 0,
					target_kind: 'project',
					target_id: projectId,
					release_id: releaseId,
					digest: '11'.repeat(32),
					applies_to: 'both',
				},
			],
			overrides: [],
			declared_time: Math.floor(Date.now() / 1000),
		}),
	);
	await page.getByRole('button', { name: 'Publish release' }).click();
	await expect(page.getByText(/Modpack manifest signed:/)).toBeVisible({ timeout: 30_000 });
	const manifestId =
		(await page.getByText(/Modpack manifest signed:/).textContent())?.split(': ').at(-1)?.trim() ??
		'';
	expect(manifestId).toMatch(/^gd:sha256:/);
	await expect(page.getByText(/Release signed:/).last()).toBeVisible({ timeout: 30_000 });
	await page.goto(`/packs/${manifestId.split(':').at(-1)}`);
	await expect(page.getByRole('heading', { name: 'Modpack manifest' })).toBeVisible();
	await expect(page.getByText(projectId)).toBeVisible();

	await page.goto(`/p/${encodeURIComponent(projectId)}`);
	await expect(page.getByRole('heading', { name: 'Browser Test Mod' })).toBeVisible();
	await expect(page.getByText('Signing roots')).toBeVisible();
	await expect(page.getByRole('link', { name: 'Download' })).toBeVisible();
	await page.getByRole('tab', { name: /Versions/ }).click();
	await expect(page.getByRole('link', { name: 'View' })).toHaveCount(1);
	await expect(page.getByRole('button', { name: 'Unfollow' })).toBeVisible();
	const releaseHref = `/p/${encodeURIComponent(projectId)}/release/${releaseId.split(':').at(-1)}`;
	await page.goto('/notifications');
	await expect(
		page.getByText(projectId.replace('gd:sha256:', '').slice(0, 12)).first(),
	).toBeVisible();
	await page.getByRole('button', { name: 'Mark read' }).first().click();
	await expect(page.getByText('read', { exact: true }).first()).toBeVisible();
	const markAll = page.getByRole('button', { name: 'Mark all read' });
	if (await markAll.isVisible()) {
		await markAll.click();
		await expect(markAll).not.toBeVisible();
	}
	await page.goto(releaseHref ?? `/p/${encodeURIComponent(projectId)}`);
	await expect(page.getByRole('heading', { name: '1.0.0' })).toBeVisible();
	await page.getByText('Check a downloaded file').click();
	await page.setInputFiles('input[aria-label="File to check against this release"]', {
		name: 'example.jar',
		mimeType: 'application/java-archive',
		buffer: Buffer.from('mod bytes'),
	});
	await expect(page.getByRole('alert').filter({ hasText: 'matches an artifact' })).toBeVisible();
	await page.setInputFiles('input[aria-label="File to check against this release"]', {
		name: 'tampered.jar',
		mimeType: 'application/java-archive',
		buffer: Buffer.from('tampered bytes'),
	});
	await expect(page.getByRole('alert').filter({ hasText: 'does not match' })).toBeVisible();
	await page.route('**/v1/artifacts/**/locations', async (route) => {
		await route.fulfill({
			contentType: 'application/json',
			body: JSON.stringify({
				protocol: 1,
				algorithm: 'sha256',
				digest: '11'.repeat(32),
				size: 9,
				locations: [
					{
						url: 'javascript:alert(document.domain)',
						kind: 'mirror',
						provenance: 'mirror-committed',
					},
				],
				refreshed_at: Math.floor(Date.now() / 1000),
			}),
		});
	});
	await page.getByRole('button', { name: 'Other locations' }).click();
	await expect(page.getByText('Unavailable mirror location')).toBeVisible();
	await expect(page.locator('a[href^="javascript:"]')).toHaveCount(0);
	await page.unroute('**/v1/artifacts/**/locations');

	await page.goto('/search?q=' + encodeURIComponent('Browser Test Mod'));
	await expect(page.getByText('Browser Test Mod')).toBeVisible();
});
