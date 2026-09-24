import { account, type Account } from '$lib/api/session';
import { home } from '$lib/home.svelte';

let current = $state<Account | null>(null);
let loaded = $state(false);
let generation = 0;
let pending: Promise<Account | null> | null = null;

export const session = {
	get user(): Account | null {
		return current;
	},
	get loaded(): boolean {
		return loaded;
	},
	get canWrite(): boolean {
		return current !== null && !home.isForeign;
	},
	get writeBlockedReason(): string | null {
		if (current === null) {
			return null;
		}
		if (!home.isForeign) {
			return null;
		}
		return `You are signed in to ${home.configured}, but this page is showing ${home.base}. Account actions stay on ${home.configured}.`;
	},
	async refresh(): Promise<Account | null> {
		if (pending) {
			return pending;
		}
		const mine = ++generation;
		pending = account()
			.catch(() => null)
			.then((user) => {
				if (mine === generation) {
					current = user;
					loaded = true;
				}
				pending = null;
				return mine === generation ? user : current;
			});
		return pending;
	},
	set(user: Account | null): void {
		generation += 1;
		pending = null;
		current = user;
		loaded = true;
	},
};
