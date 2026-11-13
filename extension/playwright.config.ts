import { defineConfig } from '@playwright/test';

// The built Chromium extension, loaded into Chromium, against local fixture
// pages and a stand-in Frog (e2e/server.mjs). Build it first: `npm run build`.
export default defineConfig({
	testDir: './e2e',
	timeout: 60_000,
	expect: { timeout: 15_000 },
	// One browser profile per test, and the stand-in Frog's record is shared.
	workers: 1,
	fullyParallel: false,
	retries: process.env.CI ? 1 : 0,
	reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : 'list',
	webServer: {
		command: 'node e2e/server.mjs',
		url: 'http://127.0.0.1:8072/index.html',
		reuseExistingServer: !process.env.CI
	}
});
