import { THEME_TOKENS, cssVariable } from '$lib/api/instance';

const STORAGE_KEY = 'moraine-color-scheme';

export const COLOR_SCHEMES = ['moraine', 'moraine-light'] as const;
export type ColorScheme = (typeof COLOR_SCHEMES)[number];

const DEFAULT_SCHEME: ColorScheme = 'moraine';

let scheme = $state<ColorScheme>(DEFAULT_SCHEME);

function isColorScheme(value: string | null): value is ColorScheme {
	return COLOR_SCHEMES.includes(value as ColorScheme);
}

function writeToDocument(next: ColorScheme): void {
	if (typeof document !== 'undefined') {
		document.documentElement.setAttribute('data-theme', next);
	}
}

export const theme = {
	get scheme(): ColorScheme {
		return scheme;
	},
	get isLight(): boolean {
		return scheme === 'moraine-light';
	},
	restore(): void {
		let stored: string | null = null;
		try {
			stored = localStorage.getItem(STORAGE_KEY);
		} catch {}
		scheme = isColorScheme(stored) ? stored : DEFAULT_SCHEME;
		writeToDocument(scheme);
	},
	set(next: ColorScheme): void {
		scheme = next;
		try {
			localStorage.setItem(STORAGE_KEY, next);
		} catch {}
		writeToDocument(next);
	},
	toggle(): void {
		theme.set(scheme === 'moraine-light' ? 'moraine' : 'moraine-light');
	},
	applyTokens(root: HTMLElement, tokens: Record<string, string>): void {
		for (const token of THEME_TOKENS) {
			const property = cssVariable(token);
			if (property) {
				root.style.removeProperty(property);
			}
		}
		for (const [token, value] of Object.entries(tokens)) {
			const property = cssVariable(token);
			if (property) {
				root.style.setProperty(property, value);
			}
		}
	},
};
