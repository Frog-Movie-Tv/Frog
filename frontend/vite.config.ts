import { sveltekit } from '@sveltejs/kit/vite';
import adapter from '@sveltejs/adapter-static';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [
		sveltekit({
			// A single-page app: the Rust server owns routing and falls back to
			// index.html, so deployment stays one binary plus one folder.
			adapter: adapter({ fallback: 'index.html', precompress: false, strict: false }),
			// Registered by hand in src/lib/offline.svelte.ts so a browser that
			// refuses reports why instead of leaving an uncaught rejection.
			serviceWorker: { register: false }
		})
	],
	server: {
		port: 5173,
		// In dev the Rust API runs separately; proxying keeps cookies same-origin.
		proxy: {
			'/api': 'http://127.0.0.1:8080',
			'/img': 'http://127.0.0.1:8080'
		}
	}
});
