import { describe, expect, it, vi } from 'vitest';

vi.mock('$env/static/public', () => ({ PUBLIC_MORAINE_REGISTRY: 'http://127.0.0.1:8080' }));

import { homeFromUrl, homeLink, isForeignHome, normalizeHome } from './home';

const url = (search: string) => new URL(`http://site.example/${search}`);

describe('normalizeHome', () => {
	it('strips trailing slashes', () => {
		expect(normalizeHome('https://mods.example.org///')).toBe('https://mods.example.org');
	});

	it('accepts loopback and public hosts', () => {
		expect(normalizeHome('http://127.0.0.1:8080')).toBe('http://127.0.0.1:8080');
		expect(normalizeHome('https://mods.example.org')).toBe('https://mods.example.org');
	});

	it('rejects an empty value', () => {
		expect(() => normalizeHome('')).toThrow();
		expect(() => normalizeHome('   ')).toThrow();
		expect(() => normalizeHome(null)).toThrow();
	});

	it('rejects schemes that are not http(s)', () => {
		expect(() => normalizeHome('file:///etc/passwd')).toThrow();
		expect(() => normalizeHome('javascript:alert(1)')).toThrow();
		expect(() => normalizeHome('data:text/html,x')).toThrow();
	});

	it('rejects private and link-local hosts', () => {
		expect(() => normalizeHome('http://10.0.0.1')).toThrow();
		expect(() => normalizeHome('http://192.168.1.1')).toThrow();
		expect(() => normalizeHome('http://169.254.169.254')).toThrow();
		expect(() => normalizeHome('http://[fd00::1]')).toThrow();
		expect(() => normalizeHome('http://[::]')).toThrow();
	});
});

describe('homeFromUrl', () => {
	it('falls back to the configured home', () => {
		expect(homeFromUrl(url(''))).toBe('http://127.0.0.1:8080');
	});

	it('prefers an explicit home parameter', () => {
		expect(homeFromUrl(url('?home=https://mods.example.org'))).toBe('https://mods.example.org');
	});

	it('ignores a blank home parameter', () => {
		expect(homeFromUrl(url('?home='))).toBe('http://127.0.0.1:8080');
	});

	it('rejects a home parameter that is not a usable URL', () => {
		expect(() => homeFromUrl(url('?home=javascript:alert(1)'))).toThrow();
		expect(() => homeFromUrl(url('?home=http://10.0.0.1'))).toThrow();
	});
});

describe('isForeignHome', () => {
	it('treats the configured home as local', () => {
		expect(isForeignHome('http://127.0.0.1:8080')).toBe(false);
	});

	it('treats any other home as foreign', () => {
		expect(isForeignHome('https://mods.example.org')).toBe(true);
	});
});

describe('isForeignHome without a configured registry', () => {
	it('treats the site itself as local and a real URL as foreign', async () => {
		vi.resetModules();
		vi.doMock('$env/static/public', () => ({ PUBLIC_MORAINE_REGISTRY: '' }));
		const sameOrigin = await import('./home');
		expect(sameOrigin.configuredHome()).toBe('');
		expect(sameOrigin.isForeignHome('')).toBe(false);
		expect(sameOrigin.isForeignHome('https://mods.example.org')).toBe(true);
		vi.doUnmock('$env/static/public');
	});
});

describe('homeLink', () => {
	it('leaves the path untouched for the local home', () => {
		expect(homeLink('/search?q=x', 'http://127.0.0.1:8080', false)).toBe('/search?q=x');
	});

	it('appends the home for a foreign registry', () => {
		expect(homeLink('/search', 'https://mods.example.org', true)).toBe(
			'/search?home=https%3A%2F%2Fmods.example.org',
		);
	});

	it('merges into an existing query instead of replacing it', () => {
		expect(homeLink('/search?q=mod&sort=updated', 'https://mods.example.org', true)).toBe(
			'/search?q=mod&sort=updated&home=https%3A%2F%2Fmods.example.org',
		);
	});

	it('replaces a stale home parameter', () => {
		expect(homeLink('/search?home=https://old.example', 'https://new.example', true)).toBe(
			'/search?home=https%3A%2F%2Fnew.example',
		);
	});
});
