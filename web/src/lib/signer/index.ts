import init, {
	genesis_roots,
	key_id,
	public_key,
	sign_changelog,
	sign_feed_entry,
	sign_genesis,
	sign_profile,
	sign_release,
	sign_modpack,
	sign_transfer,
	sign_withdrawal,
} from './pkg/moraine_wasm.js';

export type Signed = { wire: Uint8Array; id: string };

let ready: Promise<unknown> | null = null;

async function load(): Promise<void> {
	if (!ready) {
		ready = init();
	}
	await ready;
}

function parse(result: string): Signed {
	const value = JSON.parse(result) as { wire: string; id: string };
	return { wire: hexToBytes(value.wire), id: value.id };
}

export function hexToBytes(hex: string): Uint8Array {
	const clean = hex.trim();
	const bytes = new Uint8Array(clean.length / 2);
	for (let index = 0; index < bytes.length; index += 1) {
		bytes[index] = Number.parseInt(clean.slice(index * 2, index * 2 + 2), 16);
	}
	return bytes;
}

export function bytesToHex(bytes: Uint8Array): string {
	return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
}

export function randomNonce(): string {
	const bytes = new Uint8Array(16);
	crypto.getRandomValues(bytes);
	return bytesToHex(bytes);
}

export function generateSeed(): string {
	const seed = new Uint8Array(32);
	crypto.getRandomValues(seed);
	return bytesToHex(seed);
}

export function isSeed(value: string): boolean {
	return /^[0-9a-fA-F]{64}$/.test(value.trim());
}

export async function publicKey(seed: string): Promise<string> {
	await load();
	return public_key(seed.trim());
}

export async function genesisRoots(wire: Uint8Array): Promise<string[]> {
	await load();
	return JSON.parse(genesis_roots(wire)) as string[];
}

export async function keyId(seed: string): Promise<string> {
	await load();
	return key_id(seed.trim());
}

export async function signGenesis(seed: string, input: unknown): Promise<Signed> {
	await load();
	return parse(sign_genesis(seed.trim(), JSON.stringify(input)));
}

export async function signRelease(seed: string, input: unknown): Promise<Signed> {
	await load();
	return parse(sign_release(seed.trim(), JSON.stringify(input)));
}

export async function signModpack(seed: string, input: unknown): Promise<Signed> {
	await load();
	return parse(sign_modpack(seed.trim(), JSON.stringify(input)));
}

export async function signProfile(seed: string, input: unknown): Promise<Signed> {
	await load();
	return parse(sign_profile(seed.trim(), JSON.stringify(input)));
}

export async function signChangelog(seed: string, input: unknown): Promise<Signed> {
	await load();
	return parse(sign_changelog(seed.trim(), JSON.stringify(input)));
}

export async function signWithdrawal(seed: string, input: unknown): Promise<Signed> {
	await load();
	return parse(sign_withdrawal(seed.trim(), JSON.stringify(input)));
}

export async function signTransfer(
	oldSeed: string,
	newSeed: string,
	input: unknown,
): Promise<Signed> {
	await load();
	return parse(sign_transfer(oldSeed.trim(), newSeed.trim(), JSON.stringify(input)));
}

export async function signFeedEntry(seed: string, input: unknown): Promise<Signed> {
	await load();
	return parse(sign_feed_entry(seed.trim(), JSON.stringify(input)));
}
