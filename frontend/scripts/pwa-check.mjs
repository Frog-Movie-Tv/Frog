/**
 * Proves the PWA works in a real browser: the service worker registers and
 * takes control, Chrome considers the app installable, and the app keeps
 * working once the server is gone.
 *
 *   npm run pwa-check
 *
 * Uses installed Google Chrome when present, with a fresh, non-incognito
 * profile (incognito is never installable, so it would prove nothing). Runs
 * against its own throwaway server and database on port 8094.
 *
 * Offline is tested by killing the server, not by the browser's offline
 * switch: Chromium's network emulation does not apply to requests the
 * service worker makes, so the switch would let the worker quietly reach the
 * network and pass a test it should fail.
 */
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const PORT = 8094;
const BASE = `http://127.0.0.1:${PORT}`;

const binary = ['release', 'debug']
	.map((p) => join(repo, 'backend', 'target', p, process.platform === 'win32' ? 'frog.exe' : 'frog'))
	.find(existsSync);
if (!binary) {
	console.error('Build the server first.');
	process.exit(1);
}

const dataDir = join(repo, 'frontend', '.pwa-data');
const profile = mkdtempSync(join(tmpdir(), 'frog-pwa-'));
rmSync(dataDir, { recursive: true, force: true });
mkdirSync(dataDir, { recursive: true });

const server = spawn(binary, [], {
	stdio: 'ignore',
	env: {
		...process.env,
		DATABASE_URL: `sqlite:${join(dataDir, 'pwa.db').split('\\').join('/')}?mode=rwc`,
		FROG_BIND: `127.0.0.1:${PORT}`,
		FROG_STATIC_DIR: join(repo, 'frontend', 'build'),
		FROG_ADMIN_USERNAME: 'pwa',
		FROG_ADMIN_PASSWORD: 'pwa-check-password',
		TMDB_API_KEY: '',
		RUST_LOG: 'frog=warn'
	}
});

let context;
function cleanup() {
	server.kill();
	rmSync(dataDir, { recursive: true, force: true });
	rmSync(profile, { recursive: true, force: true });
}
process.on('exit', cleanup);

for (let i = 0; i < 80; i++) {
	try {
		if ((await fetch(`${BASE}/api/health`)).ok) break;
	} catch {
		/* starting */
	}
	await new Promise((r) => setTimeout(r, 250));
}

const results = [];
const check = (label, ok, detail = '') => {
	results.push(ok);
	console.log(`  [${ok ? 'PASS' : 'FAIL'}] ${label}${detail ? ' — ' + detail : ''}`);
};

const chrome = existsSync('C:/Program Files/Google/Chrome/Application/chrome.exe');
context = await chromium.launchPersistentContext(profile, {
	channel: chrome ? 'chrome' : undefined,
	headless: true
});
const page = context.pages()[0] ?? (await context.newPage());
console.log(`browser: ${chrome ? 'Google Chrome' : 'Playwright Chromium'} ${context.browser()?.version() ?? ''}`);

const consoleErrors = [];
page.on('console', (m) => m.type() === 'error' && consoleErrors.push(m.text()));
page.on('pageerror', (e) => consoleErrors.push(e.message));

await page.goto(`${BASE}/login`);
await page.getByLabel('Username').fill('pwa');
await page.getByLabel('Password').fill('pwa-check-password');
await page.getByRole('button', { name: 'Sign in' }).click();
await page.waitForURL(`${BASE}/`);

console.log('\n== service worker ==');
const state = await page.evaluate(async () => {
	const reg = await Promise.race([
		navigator.serviceWorker.ready,
		new Promise((r) => setTimeout(() => r(null), 20000))
	]);
	if (!reg) return { secure: window.isSecureContext, state: 'never became ready' };
	const worker = reg.active;
	if (worker && worker.state !== 'activated') {
		await new Promise((r) => worker.addEventListener('statechange', r, { once: true }));
	}
	return { secure: window.isSecureContext, state: reg.active?.state ?? 'none' };
});
check('secure context', state.secure);
check('worker registered and activated', state.state === 'activated', state.state);

await page.reload();
check(
	'page is controlled by the worker after reload',
	await page.evaluate(() => !!navigator.serviceWorker.controller)
);

const shellCached = await page.evaluate(async () => {
	const names = await caches.keys();
	const shell = names.find((n) => n.startsWith('frog-shell-'));
	if (!shell) return { shell: null, count: 0, root: false };
	const cache = await caches.open(shell);
	const keys = await cache.keys();
	return { shell, count: keys.length, root: !!(await cache.match('/')) };
});
check('app shell precached', shellCached.root && shellCached.count > 50, `${shellCached.count} files in ${shellCached.shell}`);

console.log('\n== installability ==');
const cdp = await context.newCDPSession(page);
const manifest = await cdp.send('Page.getAppManifest');
check('manifest found', !!manifest.url, manifest.url);
check(
	'manifest parses without errors',
	manifest.errors.length === 0,
	manifest.errors.map((e) => e.message).join('; ')
);
const install = await cdp.send('Page.getInstallabilityErrors');
check(
	'Chrome reports the app installable',
	install.installabilityErrors.length === 0,
	install.installabilityErrors.map((e) => e.errorId).join(', ') || 'no errors'
);

// Visit the pages once online so their data is cached, as a person would.
for (const path of ['/library', '/settings', '/diary', '/']) {
	await page.goto(BASE + path);
	await page.waitForTimeout(900);
}

const onlineErrors = [...consoleErrors];

console.log('\n== offline (server stopped) ==');
server.kill();
await new Promise((r) => setTimeout(r, 1200));
const reachable = await fetch(`${BASE}/api/health`).then(
	() => true,
	() => false
);
check('server is really gone', !reachable);

for (const [path, heading] of [
	['/', null],
	['/library', 'Library'],
	['/settings', 'Settings'],
	['/diary', 'Diary']
]) {
	await page.goto(BASE + path).catch(() => {});
	await page.waitForTimeout(1200);
	const title = await page.title().catch(() => '');
	const onLogin = page.url().includes('/login');
	const found = heading
		? (await page.getByRole('heading', { name: heading, level: 1 }).count()) > 0
		: !onLogin && title.length > 0;
	check(`${path} works offline`, found && !onLogin, onLogin ? 'sent to sign-in' : `title: ${title}`);
}

console.log('\n== console ==');
check(
	'no console errors while online',
	onlineErrors.length === 0,
	onlineErrors.slice(0, 3).join(' | ')
);

await context.close();
const failed = results.filter((r) => !r).length;
console.log(failed ? `\n${failed} check(s) failed` : '\nPWA checks passed');
process.exitCode = failed ? 1 : 0;
