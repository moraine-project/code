import { account, type Account } from '$lib/api/session';

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
