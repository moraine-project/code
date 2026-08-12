import { account, type Account } from '$lib/api/session';

let current = $state<Account | null>(null);
let loaded = $state(false);

export const session = {
	get user(): Account | null {
		return current;
	},
	get loaded(): boolean {
		return loaded;
	},
	async refresh(): Promise<Account | null> {
		current = await account().catch(() => null);
		loaded = true;
		return current;
	},
	set(user: Account | null): void {
		current = user;
		loaded = true;
	},
};
