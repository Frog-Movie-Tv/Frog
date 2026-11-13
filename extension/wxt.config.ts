import { defineConfig } from 'wxt';

/**
 * Where Zen and Firefox look for a newer version: the newest release's
 * `updates.json`, written by the release job. A fork sets its own.
 */
const UPDATES =
	process.env.FROG_EXTENSION_UPDATES ??
	'https://github.com/Frog-Movie-Tv/Frog/releases/latest/download/updates.json';

// One source, two browsers: `wxt build` makes the Chromium build (Brave,
// Chrome, Edge) and `wxt build -b firefox` the Firefox one (Zen, Firefox).
export default defineConfig({
	srcDir: 'src',
	modules: ['@wxt-dev/module-svelte'],
	manifestVersion: 3,
	manifest: ({ browser }) => ({
		name: 'Frog',
		description: 'Logs what you watch on any site to your Frog.',
		// Every site, because Frog cannot know in advance where you watch. The
		// content script does nothing until a video plays, and only what a
		// match needs ever leaves the browser, to your own Frog.
		host_permissions: ['<all_urls>'],
		permissions: ['storage', 'alarms', 'tabs'],
		action: { default_title: 'Frog' },
		...(browser === 'firefox'
			? {
					browser_specific_settings: {
						gecko: {
							id: 'extension@frog-movie-tv.github.io',
							strict_min_version: '142.0',
							update_url: UPDATES,
							// Watches go to the Frog you connect, never to anyone else.
							data_collection_permissions: { required: ['none'] }
						}
					}
				}
			: {})
	})
});
