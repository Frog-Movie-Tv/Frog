import { sveltekit } from '@sveltejs/kit/vite';
import adapter from '@sveltejs/adapter-static';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [
		sveltekit({
			// A single-page app: the Rust server owns routing and falls back to
			// index.html, so deployment stays one binary plus one folder.
			adapter: adapter({ fallback: 'index.html', precompress: false, strict: false })
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
