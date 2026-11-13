import { defineConfig } from 'wxt';

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
							// Watches go to the Frog you connect, never to anyone else.
							data_collection_permissions: { required: ['none'] }
						}
					}
				}
			: {})
	})
});
