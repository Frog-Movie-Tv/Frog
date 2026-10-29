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
			serviceWorker: { register: false },
			// SvelteKit boots from an inline script, which a policy of
			// script-src 'self' blocks outright — the page then renders as an
			// empty background. Hash mode puts the hash of that exact script
			// into a meta policy, so it runs and nothing else inline does.
			csp: {
				mode: 'hash',
				directives: {
					'script-src': ['self']
				}
			}
		})
	],
	build: {
		// Vite inlines files under 4 KB as data: URLs, which put the smallest
		// font subsets into the CSS as data:font, and the deployed policy
		// (font-src 'self') blocks those. Fonts always ship as files.
		assetsInlineLimit: (file) => (/\.(woff2?|ttf|otf)$/.test(file) ? false : undefined)
	},
	server: {
		port: 5173,
		// In dev the Rust API runs separately; proxying keeps cookies same-origin.
		proxy: {
			'/api': 'http://127.0.0.1:8080',
			'/img': 'http://127.0.0.1:8080'
		}
	}
});
