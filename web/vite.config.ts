import cloudflareAdapter from '@sveltejs/adapter-cloudflare';
import staticAdapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

const target = process.env.MORAINE_WEB_TARGET ?? 'static';
const adapter =
	target === 'cloudflare' ? cloudflareAdapter() : staticAdapter({ fallback: 'index.html' });

export default defineConfig({
	plugins: [
		tailwindcss(),
		sveltekit({
			csp: {
				mode: 'hash',
				directives: {
					'default-src': ['self'],
					'script-src': ['self', 'wasm-unsafe-eval'],
					'style-src': ['self', 'unsafe-inline'],
					'img-src': ['self', 'data:'],
					'connect-src': ['self', 'https:', 'http://127.0.0.1:*', 'http://localhost:*'],
					'font-src': ['self'],
					'object-src': ['none'],
					'base-uri': ['self'],
				},
			},
			compilerOptions: {
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true,
			},
			adapter,
		}),
	],
});
