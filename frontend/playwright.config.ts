import { defineConfig, devices } from '@playwright/test';

// One backend per project, each with its own throwaway database, so the
// first-run tests see a cold instance whichever project runs first.
const DESKTOP_PORT = 8099;
const MOBILE_PORT = 8098;

const server = (port: number, label: string) => ({
	command: `node scripts/e2e-server.mjs --port=${port} --label=${label}`,
	url: `http://127.0.0.1:${port}/api/health`,
	reuseExistingServer: false,
	timeout: 60_000
});

export default defineConfig({
	testDir: './e2e',
	fullyParallel: false,
	workers: 1,
	forbidOnly: !!process.env.CI,
	retries: process.env.CI ? 1 : 0,
	reporter: process.env.CI ? [['github'], ['html', { open: 'never' }]] : [['list']],
	use: {
		trace: 'on-first-retry',
		screenshot: 'only-on-failure'
	},
	projects: [
		{
			name: 'desktop',
			use: { ...devices['Desktop Chrome'], baseURL: `http://127.0.0.1:${DESKTOP_PORT}` }
		},
		{
			name: 'android',
			use: { ...devices['Pixel 7'], baseURL: `http://127.0.0.1:${MOBILE_PORT}` }
		}
	],
	webServer: [server(DESKTOP_PORT, 'desktop'), server(MOBILE_PORT, 'android')]
});
