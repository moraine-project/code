import { describe, expect, it, vi } from 'vitest';

const account = vi.fn();

vi.mock('$lib/api/session', () => ({
	account: (...args: unknown[]) => account(...args),
}));

import { session } from './session.svelte';

const user = (id: string) => ({ user_id: id, email: `${id}@example.org`, via: 'session' });

describe('session store', () => {
	it('coalesces concurrent refreshes into one request', async () => {
		account.mockReset();
		account.mockResolvedValue(user('a'));

		const [first, second] = await Promise.all([session.refresh(), session.refresh()]);

		expect(account).toHaveBeenCalledTimes(1);
		expect(first?.user_id).toBe('a');
		expect(second?.user_id).toBe('a');
		expect(session.user?.user_id).toBe('a');
	});

	it('ignores a refresh that resolves after a newer sign-in', async () => {
		account.mockReset();
		let release: ((value: unknown) => void) | undefined;
		account.mockReturnValue(new Promise((resolve) => (release = resolve)));

		const pending = session.refresh();
		session.set(user('fresh'));
		release?.(user('stale'));

		await pending;
		expect(session.user?.user_id).toBe('fresh');
	});

	it('treats a failed refresh as signed out', async () => {
		account.mockReset();
		account.mockRejectedValue(new Error('offline'));

		const result = await session.refresh();

		expect(result).toBeNull();
		expect(session.user).toBeNull();
		expect(session.loaded).toBe(true);
	});
});
