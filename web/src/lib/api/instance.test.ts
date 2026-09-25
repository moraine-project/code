import { describe, expect, it } from 'vitest';

import { cssVariable, parseInstance, safeNavHref, safeTokenValue } from './instance';
import { definitionRouteSegment } from './definitions';

const withBranding = (branding: unknown) => parseInstance({ capabilities: {}, branding });

describe('definitionRouteSegment', () => {
	it('maps the singular genesis kind the server returns to the route segment', () => {
		expect(definitionRouteSegment('game')).toBe('games');
		expect(definitionRouteSegment('loader')).toBe('loaders');
		expect(definitionRouteSegment('runtime')).toBe('runtimes');
	});

	it('refuses anything else', () => {
		expect(definitionRouteSegment('project')).toBeNull();
		expect(definitionRouteSegment('games')).toBeNull();
	});
});

describe('cssVariable', () => {
	it('maps colour tokens to daisyUI variables', () => {
		expect(cssVariable('primary')).toBe('--color-primary');
		expect(cssVariable('base-100')).toBe('--color-base-100');
		expect(cssVariable('error-content')).toBe('--color-error-content');
	});

	it('maps shape tokens to their bare names', () => {
		expect(cssVariable('radius-box')).toBe('--radius-box');
		expect(cssVariable('border')).toBe('--border');
	});

	it('refuses tokens outside the allowlist', () => {
		expect(cssVariable('display')).toBeNull();
		expect(cssVariable('background-image')).toBeNull();
		expect(cssVariable('--color-primary')).toBeNull();
		expect(cssVariable('Primary')).toBeNull();
	});
});

describe('safeTokenValue', () => {
	it('accepts hex colours in every valid length', () => {
		for (const value of ['#fff', '#ffff', '#ff0000', '#ff0000aa']) {
			expect(safeTokenValue('primary', value)).toBe(value);
		}
	});

	it('accepts colour functions with a restricted alphabet', () => {
		expect(safeTokenValue('primary', 'oklch(70% 0.15 250)')).toBe('oklch(70% 0.15 250)');
		expect(safeTokenValue('primary', 'rgb(1 2 3 / 50%)')).toBe('rgb(1 2 3 / 50%)');
	});

	it('refuses values that could escape the declaration', () => {
		expect(safeTokenValue('primary', 'red; background: url(evil)')).toBeNull();
		expect(safeTokenValue('primary', '#fff}')).toBeNull();
		expect(safeTokenValue('primary', 'url(https://evil.example/x)')).toBeNull();
		expect(safeTokenValue('primary', 'red /* }')).toBeNull();
		expect(safeTokenValue('primary', 'calc(1 + 1)')).toBeNull();
		expect(safeTokenValue('primary', '  #fff')).toBeNull();
		expect(safeTokenValue('primary', '#fff\n; color: red')).toBeNull();
	});

	it('accepts lengths for shape tokens', () => {
		expect(safeTokenValue('radius-box', '1rem')).toBe('1rem');
		expect(safeTokenValue('border', '2px')).toBe('2px');
		expect(safeTokenValue('depth', '0')).toBe('0');
		expect(safeTokenValue('noise', '1')).toBe('1');
	});

	it('refuses shapes that are not lengths', () => {
		expect(safeTokenValue('border', 'medium')).toBeNull();
		expect(safeTokenValue('radius-box', '1rem; color: red')).toBeNull();
	});

	it('refuses a colour value on a shape token and vice versa', () => {
		expect(safeTokenValue('primary', '1rem')).toBeNull();
		expect(safeTokenValue('radius-box', '#ff0000')).toBeNull();
	});
});

describe('safeNavHref', () => {
	it('accepts site-relative paths', () => {
		expect(safeNavHref('/about')).toBe('/about');
		expect(safeNavHref('/about?tab=1')).toBe('/about?tab=1');
	});

	it('rejects protocol-relative paths that would leave the origin', () => {
		expect(safeNavHref('//evil.example/path')).toBeNull();
	});

	it('accepts absolute http and https links', () => {
		expect(safeNavHref('https://docs.example')).toBe('https://docs.example');
		expect(safeNavHref('http://docs.example')).toBe('http://docs.example');
	});

	it('rejects script-bearing and non-web schemes', () => {
		expect(safeNavHref('javascript:alert(1)')).toBeNull();
		expect(safeNavHref('data:text/html,<script>alert(1)</script>')).toBeNull();
		expect(safeNavHref('vbscript:msgbox(1)')).toBeNull();
	});
});

describe('parseInstance', () => {
	it('parses the document the server actually serves', () => {
		const parsed = parseInstance({
			capabilities: {
				protocol_versions: [1],
				max_artifact_bytes: 536_870_912,
				max_upload_bytes_per_account: 5_368_709_120,
				max_feed_page_entries: 100,
				max_feed_scan_pages: 50,
				max_response_bytes: 16_777_216,
				max_sync_pages: 200,
				requests_per_minute: 600,
				max_concurrent_syncs: 4,
				maintenance_interval_seconds: 3_600,
				artifact_sources: ['local', 'external', 'mirrored'],
				upload_modes: ['staged'],
				server_role: ['home', 'directory'],
				publishing: 'open',
				registration: 'open',
				email_verification: false,
				webhook_public_key: null,
			},
			branding: { name: null, logo: null, theme: {}, nav: [] },
		});
		expect(parsed.publishing).toBe('open');
		expect(parsed.registration).toBe('open');
		expect(parsed.emailVerification).toBe(false);
		expect(parsed.protocolVersions).toEqual([1]);
		expect(parsed.maxArtifactBytes).toBe(536_870_912);
		expect(parsed.maxUploadBytesPerAccount).toBe(5_368_709_120);
		expect(parsed.maxFeedPageEntries).toBe(100);
		expect(parsed.maxFeedScanPages).toBe(50);
		expect(parsed.maxResponseBytes).toBe(16_777_216);
		expect(parsed.maxSyncPages).toBe(200);
		expect(parsed.requestsPerMinute).toBe(600);
		expect(parsed.maxConcurrentSyncs).toBe(4);
		expect(parsed.maintenanceIntervalSeconds).toBe(3_600);
		expect(parsed.artifactSources).toEqual(['local', 'external', 'mirrored']);
		expect(parsed.uploadModes).toEqual(['staged']);
		expect(parsed.serverRole).toEqual(['home', 'directory']);
		expect(parsed.webhookPublicKey).toBeNull();
		expect(parsed.branding.name).toBeNull();
		expect(parsed.branding.logo).toBeNull();
	});

	it('parses a configured document the server actually serves', () => {
		const parsed = parseInstance({
			capabilities: {
				protocol_versions: [1],
				publishing: 'progressive',
				registration: 'open',
				email_verification: true,
			},
			branding: {
				name: 'Example Mods',
				logo: 'https://cdn.example/logo.svg',
				theme: { primary: '#7c3aed', 'radius-box': '1rem' },
				nav: [{ label: 'Rules', href: '/about' }],
			},
		});
		expect(parsed.publishing).toBe('progressive');
		expect(parsed.branding.name).toBe('Example Mods');
		expect(parsed.branding.theme).toEqual({ primary: '#7c3aed', 'radius-box': '1rem' });
		expect(parsed.branding.nav).toEqual([{ label: 'Rules', href: '/about' }]);
	});

	it('falls back to safe defaults for an unusable document', () => {
		const parsed = parseInstance({ capabilities: 'nope' });
		expect(parsed.publishing).toBe('review');
		expect(parsed.registration).toBe('closed');
		expect(parsed.emailVerification).toBe(false);
		expect(parsed.maxArtifactBytes).toBeNull();
		expect(parsed.artifactSources).toEqual([]);
		expect(parsed.serverRole).toEqual([]);
		expect(parsed.branding.theme).toEqual({});
		expect(parsed.branding.nav).toEqual([]);
	});

	it('reads capabilities from the instance document', () => {
		const parsed = parseInstance({
			capabilities: {
				protocol_versions: [1, 2],
				publishing: 'progressive',
				registration: 'open',
				email_verification: true,
			},
		});
		expect(parsed.publishing).toBe('progressive');
		expect(parsed.registration).toBe('open');
		expect(parsed.emailVerification).toBe(true);
		expect(parsed.protocolVersions).toEqual([1, 2]);
	});

	it('keeps only allowlisted theme tokens with safe values', () => {
		const parsed = withBranding({
			theme: {
				primary: '#ff0000',
				'radius-box': '2rem',
				'background-image': 'url(https://evil.example/x)',
				secondary: 'red; color: blue',
			},
		});
		expect(parsed.branding.theme).toEqual({ primary: '#ff0000', 'radius-box': '2rem' });
	});

	it('keeps only nav links with a safe label and href', () => {
		const parsed = withBranding({
			nav: [
				{ label: 'Docs', href: 'https://docs.example' },
				{ label: 'About', href: '/about' },
				{ label: 'Bad', href: 'javascript:alert(1)' },
				{ label: '', href: '/empty-label' },
				{ label: 'Proto', href: '//evil.example' },
			],
		});
		expect(parsed.branding.nav).toEqual([
			{ label: 'Docs', href: 'https://docs.example' },
			{ label: 'About', href: '/about' },
		]);
	});

	it('treats a blank name as no name', () => {
		expect(withBranding({ name: '   ' }).branding.name).toBeNull();
		expect(withBranding({ name: 'Example' }).branding.name).toBe('Example');
	});
});
