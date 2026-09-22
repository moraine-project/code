import { execFileSync, spawn, type ChildProcess } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../..', import.meta.url));
const server = join(root, 'target/debug/moraine-server');
const publish = join(root, 'target/debug/moraine-publish');
const port = Number(process.env.MORAINE_E2E_PORT ?? 8137);
const reviewPort = Number(process.env.MORAINE_E2E_REVIEW_PORT ?? 8138);
const progressivePort = Number(process.env.MORAINE_E2E_PROGRESSIVE_PORT ?? 8139);
export const stateFile = join(tmpdir(), 'moraine-e2e.json');

type Instance = { child: ChildProcess; dataDir: string; password: string };

const instances: Instance[] = [];

async function waitFor(url: string) {
	const deadline = Date.now() + 30_000;
	while (Date.now() < deadline) {
		try {
			const response = await fetch(url);
			if (response.ok) {
				return;
			}
		} catch {
			// not up yet
		}
		await new Promise((ready) => setTimeout(ready, 200));
	}
	throw new Error(`the server never became ready at ${url}`);
}

export default async function globalSetup() {
	const open = startInstance(port, 'open', 'open');
	const review = startInstance(reviewPort, 'review', 'review');
	const progressive = startInstance(progressivePort, 'progressive', 'progressive');
	instances.push(open, review, progressive);
	await waitFor(`http://127.0.0.1:${port}/healthz`);
	await waitFor(`http://127.0.0.1:${reviewPort}/healthz`);
	await waitFor(`http://127.0.0.1:${progressivePort}/healthz`);
	writeFileSync(
		stateFile,
		JSON.stringify({
			baseURL: `http://127.0.0.1:${port}`,
			password: open.password,
			reviewBaseURL: `http://127.0.0.1:${reviewPort}`,
			reviewPassword: review.password,
			progressiveBaseURL: `http://127.0.0.1:${progressivePort}`,
			progressivePassword: progressive.password,
		}),
	);

	return async () => {
		for (const instance of instances) {
			instance.child.kill();
			rmSync(instance.dataDir, { recursive: true, force: true });
		}
		rmSync(stateFile, { force: true });
	};
}

function startInstance(instancePort: number, mode: string, prefix: string): Instance {
	const dataDir = mkdtempSync(join(tmpdir(), `moraine-e2e-${prefix}-`));
	const key = join(dataDir, 'definitions.key');
	execFileSync(publish, ['keygen', '--key', key], { stdio: 'ignore' });
	execFileSync(
		publish,
		[
			'define',
			'--key',
			key,
			'--dir',
			join(root, 'definitions/minecraft'),
			'--out',
			join(dataDir, 'definitions'),
		],
		{ stdio: 'ignore' },
	);
	const bootstrap = execFileSync(
		server,
		['--data-dir', dataDir, 'bootstrap', '--email', 'ops@example.org'],
		{ encoding: 'utf8' },
	);
	const password = bootstrap
		.split('\n')
		.find((line) => line.startsWith('operator password:'))
		?.split(':')[1]
		?.trim();
	if (!password) {
		throw new Error(`bootstrap did not print an operator password for ${mode}`);
	}
	const child = spawn(
		server,
		[
			'--data-dir',
			dataDir,
			'--bind',
			`127.0.0.1:${instancePort}`,
			'--web-dir',
			join(
				root,
				`web/${mode === 'review' ? 'build-review' : mode === 'progressive' ? 'build-progressive' : 'build'}`,
			),
			'--publishing',
			mode,
			'--registration',
			'open',
			'--allow-insecure-federation-local',
		],
		{ stdio: 'inherit', env: { ...process.env, MORAINE_REGISTRATION: 'open' } },
	);
	return { child, dataDir, password };
}
