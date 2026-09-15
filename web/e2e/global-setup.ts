import { execFileSync, spawn, type ChildProcess } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../..', import.meta.url));
const server = join(root, 'target/debug/moraine-server');
const publish = join(root, 'target/debug/moraine-publish');
const port = Number(process.env.MORAINE_E2E_PORT ?? 8137);
export const stateFile = join(tmpdir(), 'moraine-e2e.json');

let child: ChildProcess | undefined;
let dataDir = '';

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
	dataDir = mkdtempSync(join(tmpdir(), 'moraine-e2e-'));
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
		{
			encoding: 'utf8',
		},
	);
	const password = bootstrap
		.split('\n')
		.find((line) => line.startsWith('operator password:'))
		?.split(':')[1]
		?.trim();
	if (!password) {
		throw new Error('bootstrap did not print an operator password');
	}

	child = spawn(
		server,
		[
			'--data-dir',
			dataDir,
			'--bind',
			`127.0.0.1:${port}`,
			'--web-dir',
			join(root, 'web/build'),
			'--publishing',
			'open',
		],
		{ stdio: 'inherit' },
	);
	await waitFor(`http://127.0.0.1:${port}/healthz`);
	writeFileSync(stateFile, JSON.stringify({ baseURL: `http://127.0.0.1:${port}`, password }));

	return async () => {
		child?.kill();
		rmSync(dataDir, { recursive: true, force: true });
		rmSync(stateFile, { force: true });
	};
}
