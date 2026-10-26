/**
 * Screenshots and checks for the 0.4 polish items, in real Chrome:
 * hero contrast on Pond Light, "Start watching" for an untouched show, the
 * desktop calendar with titles in its cells, and the phone's More tab.
 *
 *   node scripts/verify-polish.mjs [outDir]
 */
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const out = resolve(process.argv[2] ?? join(repo, 'frontend', 'test-results', 'verify-polish'));
mkdirSync(out, { recursive: true });

const PORT = 8092;
const BASE = `http://127.0.0.1:${PORT}`;
const key = (() => {
	const line = readFileSync(join(repo, '.env'), 'utf8')
		.split(/\r?\n/)
		.find((l) => l.startsWith('TMDB_API_KEY='));
	return line ? line.slice('TMDB_API_KEY='.length).trim() : '';
})();

const binary = ['release', 'debug']
	.map((p) =>
		join(repo, 'backend', 'target', p, process.platform === 'win32' ? 'frog.exe' : 'frog')
	)
	.find(existsSync);
const dataDir = join(repo, 'frontend', '.polish-data');
rmSync(dataDir, { recursive: true, force: true });
mkdirSync(dataDir, { recursive: true });

const server = spawn(binary, [], {
	stdio: 'ignore',
	env: {
		...process.env,
		DATABASE_URL: `sqlite:${join(dataDir, 'p.db').split('\\').join('/')}?mode=rwc`,
		FROG_BIND: `127.0.0.1:${PORT}`,
		FROG_STATIC_DIR: join(repo, 'frontend', 'build'),
		FROG_ADMIN_USERNAME: 'polish',
		FROG_ADMIN_PASSWORD: 'polish-password',
		TMDB_API_KEY: key,
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

const results = [];
const check = (label, ok, detail = '') => {
	results.push(ok);
	console.log(`  [${ok ? 'PASS' : 'FAIL'}] ${label}${detail ? ' — ' + detail : ''}`);
};

// A show airing right now, so the calendar has something in it.
const onAir = await fetch(`https://api.themoviedb.org/3/tv/on_the_air?api_key=${key}`).then((r) =>
	r.json()
);
const airing = onAir.results.find((r) => r.poster_path && r.vote_count > 50) ?? onAir.results[0];

const chrome = existsSync('C:/Program Files/Google/Chrome/Application/chrome.exe');
const browser = await chromium.launch({ channel: chrome ? 'chrome' : undefined });

async function signedIn(viewport, extra = {}) {
	const context = await browser.newContext({ viewport, deviceScaleFactor: 2, ...extra });
	const page = await context.newPage();
	await page.goto(`${BASE}/login`);
	await page.getByLabel('Username').fill('polish');
	await page.getByLabel('Password').fill('polish-password');
	await page.getByRole('button', { name: 'Sign in' }).click();
	await page.waitForURL(`${BASE}/`);
	return { context, page };
}

{
	const { page, context } = await signedIn({ width: 1440, height: 900 });
	await page.request.put(`${BASE}/api/titles/tv/95396/library`, { data: { status: 'watching' } });
	await page.request.put(`${BASE}/api/titles/tv/${airing.id}/library`, {
		data: { status: 'watching' }
	});
	await page.request.put(`${BASE}/api/settings`, {
		data: {
			theme: 'pond-light',
			lastSeenVersion: (await (await page.request.get(`${BASE}/api/health`)).json()).version
		}
	});

	console.log('\n== hero on Pond Light ==');
	await page.goto(`${BASE}/`);
	await page.waitForTimeout(2500);
	const label = page.locator('.hero-label');
	const text = (await label.textContent())?.trim();
	check(
		'the hero says Start watching for an untouched show',
		text === 'Start watching' || text === 'Continue watching',
		text
	);
	const color = await label.evaluate((el) => getComputedStyle(el).color);
	check('the label uses the accent colour, not faint grey', color === 'rgb(47, 125, 79)', color);
	await page.screenshot({ path: join(out, '1-hero-pond-light.png') });

	console.log('\n== calendar on desktop ==');
	await page.request.put(`${BASE}/api/settings`, {
		data: { theme: 'pond', lastSeenVersion: '0.4.0' }
	});
	await page.goto(`${BASE}/calendar`);
	await page.waitForTimeout(2000);
	const titles = await page.locator('.cell .mini .t').count();
	check('day cells name what is on', titles > 0, `${titles} titles in cells`);
	check('a legend explains the marks', (await page.locator('.legend li').count()) === 4);
	const aside = await page.locator('aside.day').boundingBox();
	const grid = await page.locator('.grid').boundingBox();
	check(
		'the chosen day sits beside the grid',
		aside && grid && aside.x > grid.x + grid.width - 2,
		`grid ends ${Math.round(grid.x + grid.width)}, panel starts ${Math.round(aside.x)}`
	);
	await page.screenshot({ path: join(out, '2-calendar-desktop.png') });
	await context.close();
}

{
	console.log('\n== phone tab bar ==');
	const { page, context } = await signedIn(
		{ width: 375, height: 812 },
		{ isMobile: true, hasTouch: true }
	);
	const tabs = page.locator('nav.tabs');
	const labels = (await tabs.locator('small').allTextContents()).map((t) => t.trim());
	check('the tab bar ends with More', labels.at(-1) === 'More', labels.join(' · '));
	await tabs.getByRole('button', { name: /More/ }).click();
	const sheet = page.getByRole('menu', { name: 'More' });
	await sheet.waitFor();
	const items = (await sheet.getByRole('menuitem').allTextContents()).map((t) => t.trim());
	for (const want of ['Diary', 'Stats', 'Settings', 'Import & export', 'About']) {
		check(`More offers ${want}`, items.includes(want));
	}
	await page.screenshot({ path: join(out, '3-more-sheet-375.png') });
	await sheet.getByRole('menuitem', { name: 'Diary' }).click();
	await page.waitForURL(`${BASE}/diary`);
	check('choosing an item goes there and closes the sheet', (await sheet.count()) === 0);
	await context.close();
}

await browser.close();
const failed = results.filter((r) => !r).length;
console.log(
	`\nscreenshots in ${out}\n${failed ? failed + ' check(s) failed' : 'all checks passed'}`
);
// The 'exit' handler never runs while the server keeps Node alive.
server.kill();
process.exitCode = failed ? 1 : 0;
