import { describe, expect, it, vi } from 'vitest';

vi.mock('$env/static/public', () => ({ PUBLIC_MORAINE_REGISTRY: 'http://127.0.0.1:8080' }));

import { sameOrigin, sessionBase } from './session';

describe('sameOrigin', () => {
	it('treats the exact origin as same', () => {
		expect(sameOrigin('http://127.0.0.1:8080', 'http://127.0.0.1:8080')).toBe(true);
	});

	it('treats localhost and 127.0.0.1 on one port as same', () => {
		expect(sameOrigin('http://127.0.0.1:8080', 'http://localhost:8080')).toBe(true);
		expect(sameOrigin('http://localhost:8080', 'http://127.0.0.1:8080')).toBe(true);
	});

	it('keeps a different port or host cross-origin', () => {
		expect(sameOrigin('http://127.0.0.1:8080', 'http://127.0.0.1:4173')).toBe(false);
		expect(sameOrigin('https://api.example', 'https://site.example')).toBe(false);
	});
});

describe('apiBase', () => {
	it('uses a relative base when the page is the configured home', () => {
		vi.stubGlobal('window', { location: { origin: 'http://127.0.0.1:8080' } });
		expect(sessionBase()).toBe('');
	});

	it('uses a relative base across loopback aliases', () => {
		vi.stubGlobal('window', { location: { origin: 'http://localhost:8080' } });
		expect(sessionBase()).toBe('');
	});

	it('uses the configured base on a different origin', () => {
		vi.stubGlobal('window', { location: { origin: 'https://site.example' } });
		expect(sessionBase()).toBe('http://127.0.0.1:8080');
	});
});
