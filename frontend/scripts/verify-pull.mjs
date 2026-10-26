/**
 * Verifies pull-to-refresh with real touch input in Chrome:
 *
 *   1. on a touch screen, pulling down from the top past the threshold reloads,
 *   2. a short pull does not, and the indicator goes away,
 *   3. a pull that starts inside the More sheet leaves the page alone,
 *   4. a desktop keeps the browser's own scrolling (no overscroll override).
 *
 *   node scripts/verify-pull.mjs     (needs a release build)
 */
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const PORT = 8085;
const BASE = `http://127.0.0.1:${PORT}`;
const USER = ['pull', 'pull-password-1'];

const results = [];
const check = (label, ok, detail = '') => {
	results.push(!!ok);
	console.log(`  [${ok ? 'PASS' : 'FAIL'}] ${label}${detail ? ' — ' + detail : ''}`);
};

const binary = join(
	repo,
	'backend',
	'target',
	'release',
	process.platform === 'win32' ? 'frog.exe' : 'frog'
);
const dataDir = join(repo, 'frontend', '.pull-data');
rmSync(dataDir, { recursive: true, force: true });
mkdirSync(dataDir, { recursive: true });
const server = spawn(binary, [], {
	stdio: 'ignore',
	env: {
		...process.env,
		DATABASE_URL: `sqlite:${join(dataDir, 'p.db').split('\\').join('/')}?mode=rwc`,
		FROG_BIND: `127.0.0.1:${PORT}`,
		FROG_STATIC_DIR: join(repo, 'frontend', 'build'),
		FROG_ADMIN_USERNAME: USER[0],
		FROG_ADMIN_PASSWORD: USER[1],
		TMDB_API_KEY: '',
		RUST_LOG: 'frog=warn'
	}
});
process.on('exit', () => {
	server.kill();
	rmSync(dataDir, { recursive: true, force: true });
});
for (let i = 0; i < 80; i++) {
	try {
		if ((await fetch(`${BASE}/api/health`)).ok) break;
	} catch {
		/* starting */
	}
	await new Promise((r) => setTimeout(r, 250));
}

const chrome = existsSync('C:/Program Files/Google/Chrome/Application/chrome.exe');
const browser = await chromium.launch({ channel: chrome ? 'chrome' : undefined });

async function signedIn(options) {
	const context = await browser.newContext(options);
	const page = await context.newPage();
	await page.goto(`${BASE}/login`);
	await page.getByLabel('Username').fill(USER[0]);
	await page.getByLabel('Password').fill(USER[1]);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await page.waitForURL(/\/(welcome)?$/);
	// Welcomed already, so the checks below run on Library.
	await page.evaluate(() =>
		fetch('/api/settings', {
			method: 'PUT',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify({ onboarded: true })
		})
	);
	return { context, page };
}

console.log('== phone ==');
const { context, page } = await signedIn({
	viewport: { width: 375, height: 812 },
	isMobile: true,
	hasTouch: true
});
await page.goto(`${BASE}/library`);
await page.getByRole('heading', { name: 'Library', level: 1 }).waitFor();
check(
	'native overscroll is handed over',
	(await page.evaluate(() => document.documentElement.style.overscrollBehaviorY)) === 'contain'
);

const cdp = await context.newCDPSession(page);
async function drag(fromY, toY, x = 187) {
	await cdp.send('Input.dispatchTouchEvent', {
		type: 'touchStart',
		touchPoints: [{ x, y: fromY, id: 1 }]
	});
	for (let i = 1; i <= 12; i++) {
		await cdp.send('Input.dispatchTouchEvent', {
			type: 'touchMove',
			touchPoints: [{ x, y: fromY + ((toY - fromY) * i) / 12, id: 1 }]
		});
	}
}
const release = () => cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
const mark = () => page.evaluate(() => (window.__frogMark = 1));
const marked = () => page.evaluate(() => window.__frogMark === 1).catch(() => false);

await mark();
await drag(150, 200);
check('a pull shows the indicator', (await page.locator('.ptr').count()) === 1);
await release();
await page.waitForTimeout(600);
check('a short pull does not reload', await marked());
check('and the indicator goes away', (await page.locator('.ptr').count()) === 0);

await drag(150, 340);
check('a long pull says release', (await page.locator('.ptr.ready').count()) === 1);
await page.screenshot({ path: join(repo, 'frontend', 'test-results', 'pull-ready.png') });
await Promise.all([page.waitForEvent('load'), release()]);
check('letting go reloads the page', !(await marked()));
await page.getByRole('heading', { name: 'Library', level: 1 }).waitFor();

await mark();
await page.getByRole('button', { name: 'More' }).click();
const sheet = page.getByRole('menu', { name: 'More' });
await sheet.waitFor();
const box = await sheet.boundingBox();
await drag(box.y + 20, box.y + 220);
await release();
await page.waitForTimeout(600);
check('a pull inside the More sheet leaves the page alone', await marked());
await context.close();

console.log('\n== desktop ==');
const desk = await signedIn({ viewport: { width: 1280, height: 900 } });
await desk.page.goto(`${BASE}/library`);
await desk.page.getByRole('heading', { name: 'Library', level: 1 }).waitFor();
check(
	'a mouse keeps the browser scrolling',
	(await desk.page.evaluate(() => document.documentElement.style.overscrollBehaviorY)) === ''
);
await desk.context.close();

await browser.close();
server.kill();
const failed = results.filter((r) => !r).length;
console.log(`\n${failed ? failed + ' check(s) failed' : 'all checks passed'}`);
process.exitCode = failed ? 1 : 0;
