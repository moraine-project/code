import { instance } from '$lib/instance.svelte';

export function pageTitle(suffix?: string): string {
	return suffix ? `${suffix} · ${instance.name}` : instance.name;
}
