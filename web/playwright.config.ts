import { defineConfig } from '@playwright/test';

const port = Number(process.env.MORAINE_E2E_PORT ?? 8137);

export default defineConfig({
	testDir: './e2e',
	globalSetup: './e2e/global-setup.ts',
	timeout: 60_000,
	fullyParallel: false,
	workers: 1,
	reporter: 'list',
	use: {
		baseURL: `http://127.0.0.1:${port}`,
		trace: 'on-first-retry',
	},
	projects: [{ name: 'chromium', use: { browserName: 'chromium' } }],
});
