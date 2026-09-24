import { z } from 'zod';

import { normalizeHome } from '../home';
import { safeExternalUrl } from './external-url';
import type { Fetcher } from './request';
import { registryUrl } from './request';

const COLOR_TOKENS = [
	'base-100',
	'base-200',
	'base-300',
	'base-content',
	'primary',
	'primary-content',
	'secondary',
	'secondary-content',
	'accent',
	'accent-content',
	'neutral',
	'neutral-content',
	'info',
	'info-content',
	'success',
	'success-content',
	'warning',
	'warning-content',
	'error',
	'error-content',
] as const;

const SHAPE_TOKENS = [
	'radius-selector',
	'radius-field',
	'radius-box',
	'size-selector',
	'size-field',
	'border',
	'depth',
	'noise',
] as const;

const HEX = /^#(?:[0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i;
const COLOR_FUNCTION =
	/^(?:rgb|rgba|hsl|hsla|hwb|lab|lch|oklab|oklch|color)\([0-9a-z%.,/+\- ]*\)$/i;
const LENGTH = /^\d+(?:\.\d+)?(?:px|rem|em|%)?$/;

export const THEME_TOKENS: readonly string[] = [...COLOR_TOKENS, ...SHAPE_TOKENS];

export function cssVariable(token: string): string | null {
	if ((COLOR_TOKENS as readonly string[]).includes(token)) {
		return `--color-${token}`;
	}
	if ((SHAPE_TOKENS as readonly string[]).includes(token)) {
		return `--${token}`;
	}
	return null;
}

export function safeTokenValue(token: string, value: string): string | null {
	const trimmed = value.trim();
	if (trimmed !== value || trimmed.length === 0 || trimmed.length > 64) {
		return null;
	}
	if ((COLOR_TOKENS as readonly string[]).includes(token)) {
		return HEX.test(trimmed) || COLOR_FUNCTION.test(trimmed) ? trimmed : null;
	}
	return LENGTH.test(trimmed) ? trimmed : null;
}

export function safeNavHref(href: string): string | null {
	const trimmed = href.trim();
	if (trimmed === href && trimmed.startsWith('/') && !trimmed.startsWith('//')) {
		return trimmed;
	}
	return safeExternalUrl(trimmed);
}

export type NavLink = { label: string; href: string };
export type Theme = Record<string, string>;

export type Branding = {
	name: string | null;
	logo: string | null;
	theme: Theme;
	nav: NavLink[];
};

export type Instance = {
	publishing: string;
	registration: string;
	emailVerification: boolean;
	protocolVersions: number[];
	branding: Branding;
};

export const defaultInstance: Instance = {
	publishing: 'review',
	registration: 'closed',
	emailVerification: false,
	protocolVersions: [1],
	branding: { name: null, logo: null, theme: {}, nav: [] },
};

const rawInstanceSchema = z.object({
	capabilities: z
		.object({
			protocol_versions: z.array(z.number()).optional(),
			publishing: z.string().optional(),
			registration: z.string().optional(),
			email_verification: z.boolean().optional(),
		})
		.optional(),
	branding: z
		.object({
			name: z.string().nullish(),
			logo: z.string().nullish(),
			theme: z.record(z.string(), z.string()).nullish(),
			nav: z.array(z.object({ label: z.string().nullish(), href: z.string().nullish() })).nullish(),
		})
		.nullish(),
});

function parseBranding(value: z.infer<typeof rawInstanceSchema>['branding']): Branding {
	if (!value) {
		return defaultInstance.branding;
	}
	const theme: Theme = {};
	for (const [token, raw] of Object.entries(value.theme ?? {})) {
		const property = cssVariable(token);
		const safe = safeTokenValue(token, raw);
		if (property && safe) {
			theme[token] = safe;
		}
	}
	const nav: NavLink[] = [];
	for (const link of value.nav ?? []) {
		const href = safeNavHref(link.href ?? '');
		const label = (link.label ?? '').trim();
		if (href && label && label.length <= 64) {
			nav.push({ label, href });
		}
	}
	return {
		name: value.name?.trim() || null,
		logo: value.logo?.trim() || null,
		theme,
		nav,
	};
}

export function parseInstance(payload: unknown): Instance {
	const parsed = rawInstanceSchema.safeParse(payload);
	if (!parsed.success) {
		return defaultInstance;
	}
	const capabilities = parsed.data.capabilities;
	return {
		publishing: capabilities?.publishing ?? defaultInstance.publishing,
		registration: capabilities?.registration ?? defaultInstance.registration,
		emailVerification: capabilities?.email_verification ?? false,
		protocolVersions: capabilities?.protocol_versions ?? [1],
		branding: parseBranding(parsed.data.branding),
	};
}

export async function fetchInstance(base: string, fetchFn: Fetcher = fetch): Promise<Instance> {
	const response = await fetchFn(registryUrl(base, '/v1/instance'));
	if (!response.ok) {
		throw new Error(`home returned ${response.status} for the instance document`);
	}
	return parseInstance(await response.json());
}

export function instanceCapabilitiesUrl(base: string): string {
	return `${normalizeHome(base)}/.well-known/mod-registry`;
}
