/**
 * Verifies the stats page and the year in review in real Chrome, against
 * real TMDB data:
 *
 *   1. films imported with dates, a film marked watched in the app, and a
 *      week of dated episodes all count,
 *   2. people come from stored credits (Christopher Nolan directs three),
 *   3. picking a year switches to months and keeps the year in the URL,
 *   4. the year in review renders a 1080x1920 PNG with real posters and can
 *      be saved,
 *   5. the page fits a 375px phone without sideways scrolling.
 *
 *   node scripts/verify-stats.mjs     (needs the TMDB key and a release build)
 */
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const out = join(repo, 'frontend', 'test-results', 'verify-stats');
mkdirSync(out, { recursive: true });

const PORT = 8097;
const BASE = `http://127.0.0.1:${PORT}`;
const USER = 'stats';
const PASS = 'stats-password';
const key = readFileSync(join(repo, '.env'), 'utf8')
	.split(/\r?\n/)
	.find((l) => l.startsWith('TMDB_API_KEY='))
	?.slice('TMDB_API_KEY='.length)
	.trim();
if (!key) {
	console.error('TMDB_API_KEY is empty in .env; this check needs real metadata.');
	process.exit(1);
}

const results = [];
const check = (label, ok, detail = '') => {
	results.push(!!ok);
	console.log(`  [${ok ? 'PASS' : 'FAIL'}] ${label}${detail ? ' — ' + detail : ''}`);
};

const binary = join(repo, 'backend', 'target', 'release', process.platform === 'win32' ? 'frog.exe' : 'frog');
const dataDir = join(repo, 'frontend', '.stats-data');
rmSync(dataDir, { recursive: true, force: true });
mkdirSync(dataDir, { recursive: true });

const server = spawn(binary, [], {
	stdio: 'ignore',
	env: {
		...process.env,
		DATABASE_URL: `sqlite:${join(dataDir, 's.db').split('\\').join('/')}?mode=rwc`,
		FROG_BIND: `127.0.0.1:${PORT}`,
		FROG_STATIC_DIR: join(repo, 'frontend', 'build'),
		FROG_ADMIN_USERNAME: USER,
		FROG_ADMIN_PASSWORD: PASS,
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

// --- seed through the real API ----------------------------------------------
const login = await fetch(`${BASE}/api/auth/login`, {
	method: 'POST',
	headers: { 'content-type': 'application/json' },
	body: JSON.stringify({ username: USER, password: PASS, remember: true })
});
const cookie = login.headers.get('set-cookie')?.split(';')[0];
const call = async (method, path, body) => {
	const res = await fetch(`${BASE}${path}`, {
		method,
		headers: { 'content-type': 'application/json', cookie },
		body: body === undefined ? undefined : JSON.stringify(body)
	});
	const json = await res.json().catch(() => null);
	if (!res.ok) throw new Error(`${method} ${path}: ${res.status} ${JSON.stringify(json)}`);
	return json;
};

console.log('== seeding ==');
const films = [
	[278, '2025-03-14', 95],
	[680, '2025-06-02', 88],
	[155, '2025-07-19', 90],
	[157336, '2025-11-03', 92],
	[603, '2025-11-04', 80],
	[27205, '2026-01-10', 85]
];
const imported = await call('POST', '/api/import/apply', {
	on_conflict: 'overwrite',
	items: films.map(([id, day, rating]) => ({
		kind: 'movie',
		tmdb_id: id,
		status: 'completed',
		rating,
		watched_at: `${day}T20:00:00Z`
	}))
});
check('six films import', imported.added === 6, JSON.stringify(imported));

// Breaking Bad S1E1-7 on consecutive days right after the two films: a
// nine-day streak from 3 to 11 November.
for (let e = 1; e <= 7; e++) {
	await call('POST', `/api/titles/tv/1396/episodes/1/${e}/watch`, {
		watched: true,
		watched_at: `2025-11-${String(4 + e).padStart(2, '0')}T21:00:00Z`
	});
}
// A film finished in the app rather than imported.
await call('PUT', '/api/titles/movie/438631/library', { status: 'completed' });

const all = await call('GET', '/api/stats');
check('every film counts, the one marked in the app included', all.totals.films === 7, `films ${all.totals.films}`);
check('episodes count', all.totals.episodes === 7, `episodes ${all.totals.episodes}`);
check('the November run is the longest streak', all.streaks.longest.days === 9, JSON.stringify(all.streaks.longest));
check('a film watched today starts a current streak', all.streaks.current.days >= 1);
check('Nolan leads directors', all.people.directors[0]?.name === 'Christopher Nolan' && all.people.directors[0]?.titles === 3, JSON.stringify(all.people.directors[0]));
check('an actor shared by several films leads actors', all.people.actors[0]?.titles >= 2, JSON.stringify(all.people.actors[0]));
check('studios are ranked', all.people.studios.length > 0, all.people.studios[0]?.name);
check('no credits are left to fetch', all.credits_pending === 0, String(all.credits_pending));

// --- the page in Chrome --------------------------------------------------------
const chrome = existsSync('C:/Program Files/Google/Chrome/Application/chrome.exe');
const browser = await chromium.launch({ channel: chrome ? 'chrome' : undefined });

async function signedIn(viewport, extra = {}) {
	const context = await browser.newContext({ viewport, acceptDownloads: true, ...extra });
	const page = await context.newPage();
	const errors = [];
	page.on('pageerror', (e) => errors.push(e.message));
	page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
	await page.goto(`${BASE}/login`);
	await page.getByLabel('Username').fill(USER);
	await page.getByLabel('Password').fill(PASS);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await page.waitForURL(`${BASE}/`);
	return { page, context, errors };
}

{
	console.log('\n== desktop ==');
	const { page, context, errors } = await signedIn({ width: 1280, height: 900 });
	await page.goto(`${BASE}/stats`);
	await page.getByRole('heading', { name: 'Watch time by year' }).waitFor();
	check('the tab is named', (await page.title()) === 'Stats · Frog');
	for (const name of ['Most watched shows', 'Top films', 'Genres', 'Ratings', 'Release decades', 'Actors', 'Directors and creators', 'Studios', 'Library']) {
		check(`shows ${name}`, (await page.getByRole('heading', { name, exact: true }).count()) === 1);
	}
	const directors = page.locator('section', { has: page.getByRole('heading', { name: 'Directors and creators' }) });
	check('Christopher Nolan is first, linked to his page', (await directors.locator('a.person').first().getAttribute('href')) === `/person/${all.people.directors[0].tmdb_id}`);
	// Headshots are lazy and come through the proxy from TMDB on first view.
	await page.locator('.people').last().scrollIntoViewIfNeeded();
	await page
		.waitForFunction(() => [...document.querySelectorAll('.people img')].every((i) => i.complete), null, { timeout: 30000 })
		.catch(() => undefined);
	for (const name of ['Actors', 'Directors and creators', 'Studios']) {
		const section = page.locator('section', { has: page.getByRole('heading', { name, exact: true }) });
		const imgs = await section.locator('img').evaluateAll((list) => list.map((i) => i.naturalWidth > 0));
		check(`${name} images load through the proxy`, imgs.length > 0 && imgs.every(Boolean), `${imgs.filter(Boolean).length}/${imgs.length}`);
	}
	await page.evaluate(() => window.scrollTo(0, 0));
	await page.screenshot({ path: join(out, '1-all-time-desktop.png'), fullPage: true });

	await page.getByRole('button', { name: 'Period' }).click();
	await page.getByRole('option', { name: '2025' }).click();
	await page.waitForURL(/year=2025/);
	await page.getByRole('heading', { name: 'Watch time by month' }).waitFor();
	const months = await page.locator('section', { has: page.getByRole('heading', { name: 'Watch time by month' }) }).locator('.column').count();
	check('a year is drawn month by month', months === 12, `${months} columns`);
	const filmsTile = await page.locator('.tile', { hasText: 'Films' }).locator('strong').textContent();
	check('the year counts its own films', filmsTile?.trim() === '5', `films tile ${filmsTile}`);
	check('the library breakdown stays with all time', (await page.getByRole('heading', { name: 'Library', exact: true }).count()) === 0);
	await page.screenshot({ path: join(out, '2-year-desktop.png'), fullPage: true });

	await page.reload();
	await page.getByRole('heading', { name: 'Watch time by month' }).waitFor();
	check('the year survives a reload', page.url().includes('year=2025'));

	console.log('\n== year in review ==');
	const posters = [];
	page.on('response', (r) => r.url().includes('/img/w342/') && posters.push(r.status()));
	await page.getByRole('button', { name: '2025 in review' }).click();
	const dialog = page.getByRole('dialog', { name: '2025 in review' });
	const image = dialog.locator('img');
	await image.waitFor({ timeout: 20000 });
	await page.waitForFunction(() => document.querySelector('[role=dialog] img')?.complete);
	const size = await image.evaluate((i) => [i.naturalWidth, i.naturalHeight]);
	check('the card is 1080 by 1920', size[0] === 1080 && size[1] === 1920, size.join('x'));
	check('posters were drawn from the proxy', posters.length >= 3 && posters.every((s) => s === 200), posters.join(','));
	const png = await image.evaluate(async (i) => {
		const b = await (await fetch(i.src)).blob();
		const bytes = new Uint8Array(await b.arrayBuffer());
		let s = '';
		for (const x of bytes) s += String.fromCharCode(x);
		return btoa(s);
	});
	writeFileSync(join(out, 'review-2025.png'), Buffer.from(png, 'base64'));
	const [download] = await Promise.all([page.waitForEvent('download'), dialog.getByRole('button', { name: /Save/ }).click()]);
	check('saving downloads a named PNG', download.suggestedFilename() === 'frog-2025-in-review.png', download.suggestedFilename());
	await page.screenshot({ path: join(out, '3-review-dialog.png') });
	await page.keyboard.press('Escape');
	const closed = await dialog.waitFor({ state: 'detached', timeout: 2000 }).then(() => true, () => false);
	check('Escape closes it', closed);

	check('no errors in the console', errors.length === 0, errors.join(' | '));
	await context.close();
}

{
	console.log('\n== phone ==');
	const { page, context, errors } = await signedIn({ width: 375, height: 812 }, { isMobile: true, hasTouch: true });
	for (const path of ['/stats', '/stats?year=2025']) {
		await page.goto(`${BASE}${path}`);
		await page.getByRole('heading', { name: /Watch time by/ }).waitFor();
		const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
		check(`${path} fits 375px`, overflow <= 0, `${overflow}px too wide`);
	}
	await page.screenshot({ path: join(out, '4-year-375.png'), fullPage: true });
	await page.getByRole('button', { name: '2025 in review' }).click();
	await page.getByRole('dialog').locator('img').waitFor({ timeout: 20000 });
	const box = await page.getByRole('dialog').boundingBox();
	check('the review dialog fits the phone', box && box.width <= 375 && box.y >= 0 && box.y + box.height <= 812, JSON.stringify(box));
	await page.screenshot({ path: join(out, '5-review-375.png') });
	check('no errors in the console', errors.length === 0, errors.join(' | '));
	await context.close();
}

await browser.close();
server.kill();
const failed = results.filter((r) => !r).length;
console.log(`\nscreenshots in ${out}\n${failed ? failed + ' check(s) failed' : 'all checks passed'}`);
process.exitCode = failed ? 1 : 0;
