/**
 * Verifies AniList sync in real Chrome, against real TMDB (AniList itself is
 * a fixture, so the list can change between runs):
 *
 *   1. "Sync now" in Settings adds the list, with seasons folded the way the
 *      Import page folds them, and leaves a title it cannot place alone,
 *   2. opening the app after six hours syncs again: progress moves forward,
 *      while a rating and a pause set in Frog stay,
 *   3. opening it again soon after does nothing, and a sync with nothing new
 *      says so,
 *   4. the Settings row fits a phone.
 *
 *   node scripts/verify-anilist-sync.mjs     (needs the TMDB key and a release build)
 */
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const out = join(repo, 'frontend', 'test-results', 'verify-anilist-sync');
mkdirSync(out, { recursive: true });

const PORT = 8088;
const BASE = `http://127.0.0.1:${PORT}`;
const USER = 'sync';
const PASS = 'sync-password';
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

const binary = join(
	repo,
	'backend',
	'target',
	'release',
	process.platform === 'win32' ? 'frog.exe' : 'frog'
);
const dataDir = join(repo, 'frontend', '.anilist-sync-data');
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
	// Windows can hold the database a moment after the kill; the next run clears it.
	try {
		rmSync(dataDir, { recursive: true, force: true });
	} catch {
		/* still locked */
	}
});
for (let i = 0; i < 80; i++) {
	try {
		if ((await fetch(`${BASE}/api/health`)).ok) break;
	} catch {
		/* starting */
	}
	await new Promise((r) => setTimeout(r, 250));
}

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
// Skip the welcome so the app lands on Home.
await call('PUT', '/api/settings', { onboarded: true });

const none = { year: null, month: null, day: null };
const entry = (status, score, progress, title, year, episodes, completedAt = none) => ({
	status,
	score,
	progress,
	completedAt,
	media: {
		format: 'TV',
		episodes,
		seasonYear: year,
		startDate: { year },
		title: { english: title, romaji: title },
		synonyms: []
	}
});

/** What AniList answers; swapped between syncs. */
let list = [
	entry('COMPLETED', 80, 24, 'JUJUTSU KAISEN', 2020, 24, { year: 2021, month: 3, day: 27 }),
	entry('CURRENT', 85, 5, 'JUJUTSU KAISEN Season 2', 2023, 23),
	entry('PLANNING', 0, 0, 'Samurai Champloo', 2004, 26),
	entry('CURRENT', 0, 3, 'Qwzx Nothing By This Name', 1999, 12)
];
let asked = 0;

const chrome = existsSync('C:/Program Files/Google/Chrome/Application/chrome.exe');
const browser = await chromium.launch({ channel: chrome ? 'chrome' : undefined });
const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
await context.route('https://graphql.anilist.co/**', (route) => {
	asked++;
	return route.fulfill({
		contentType: 'application/json',
		body: JSON.stringify({ data: { MediaListCollection: { lists: [{ entries: list }] } } })
	});
});
const page = await context.newPage();
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));
page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
await page.goto(`${BASE}/login`);
await page.getByLabel('Username').fill(USER);
await page.getByLabel('Password').fill(PASS);
await page.getByRole('button', { name: 'Sign in' }).click();
await page.waitForURL(`${BASE}/`);

const find = async (q) => (await call('GET', `/api/library?q=${encodeURIComponent(q)}`)).items;
const toast = (text) => page.locator('.toast, [role="status"]').filter({ hasText: text });

console.log('== Sync now ==');
check('nothing syncs before a list is linked', asked === 0, `${asked} requests`);
await page.goto(`${BASE}/settings`);
const row = page.locator('.anilist');
await row.scrollIntoViewIfNeeded();
await row.getByLabel('AniList username').fill('someone');
await row.getByRole('button', { name: 'Sync now' }).click();
await toast(/from AniList/)
	.first()
	.waitFor({ timeout: 120000 });
const firstToast = (
	await toast(/from AniList/)
		.first()
		.textContent()
)?.trim();
check('the toast says what came across', /2 added/.test(firstToast ?? ''), firstToast);
check(
	'a title it cannot place is left for Import',
	(await row.getByText('1 title needs a choice first').count()) === 1
);
check(
	'the row says when it last synced',
	(await row.getByText(/Synced today with someone/).count()) === 1
);
await page.screenshot({ path: join(out, '1-settings-synced.png') });

let jjk = (await find('Jujutsu'))[0];
check('Jujutsu Kaisen is in the library', !!jjk);
check('watching, from its latest season', jjk?.entry?.status === 'watching', jjk?.entry?.status);
check('24 + 5 episodes ticked', jjk?.watched_episodes === 29, `${jjk?.watched_episodes} watched`);
let champloo = (await find('Champloo'))[0];
check(
	'Samurai Champloo is on the watchlist',
	champloo?.entry?.status === 'planned',
	champloo?.entry?.status
);
check('nothing invented for the unknown title', (await find('Qwzx')).length === 0);

console.log('\n== Opening the app later ==');
// Your own changes in Frog.
await call('PUT', `/api/titles/tv/${jjk.tmdb_id}/library`, { rating: 95 });
await call('PUT', `/api/titles/tv/${champloo.tmdb_id}/library`, { status: 'paused' });
// AniList moves on.
list = [
	list[0],
	entry('CURRENT', 60, 10, 'JUJUTSU KAISEN Season 2', 2023, 23),
	entry('COMPLETED', 90, 26, 'Samurai Champloo', 2004, 26, { year: 2024, month: 5, day: 1 }),
	list[3]
];
// Six hours on.
const prefs = await call('GET', '/api/settings');
const stale = new Date(Date.now() - 7 * 60 * 60 * 1000).toISOString();
await call('PUT', '/api/settings', { ...prefs, anilistSyncedAt: stale });

const before = asked;
await page.goto(`${BASE}/`);
await toast(/updated from AniList/)
	.first()
	.waitFor({ timeout: 120000 });
check('opening the app synced once', asked === before + 1, `${asked - before} requests`);
await page.screenshot({ path: join(out, '2-home-toast.png') });

jjk = (await find('Jujutsu'))[0];
check('progress moved forward', jjk?.watched_episodes === 34, `${jjk?.watched_episodes} watched`);
check('your rating stays', jjk?.entry?.rating === 95, String(jjk?.entry?.rating));
champloo = (await find('Champloo'))[0];
check('your pause stays', champloo?.entry?.status === 'paused', champloo?.entry?.status);
check('an empty rating is filled', champloo?.entry?.rating === 90, String(champloo?.entry?.rating));

console.log('\n== Soon after ==');
const settled = asked;
await page.reload();
await page.waitForLoadState('networkidle');
await page.waitForTimeout(1500);
check('a fresh sync is not repeated on open', asked === settled, `${asked - settled} requests`);

await page.goto(`${BASE}/settings`);
await row.scrollIntoViewIfNeeded();
check(
	'the username is remembered',
	(await row.getByLabel('AniList username').inputValue()) === 'someone'
);
await row.getByRole('button', { name: 'Sync now' }).click();
await toast('In step with AniList').first().waitFor({ timeout: 120000 });
check('a sync with nothing new says so', true);

console.log('\n== Stop ==');
await row.getByRole('button', { name: 'Stop' }).click();
await toast('AniList sync is off').first().waitFor();
await page.waitForTimeout(800);
const off = await call('GET', '/api/settings');
check('stopping forgets the list', !off.anilistUser && !off.anilistSyncedAt);

console.log('\n== Phone ==');
const phone = await browser.newContext({
	viewport: { width: 412, height: 915 },
	deviceScaleFactor: 2.6,
	isMobile: true,
	hasTouch: true
});
await phone.addCookies([
	{ name: cookie.split('=')[0], value: cookie.split('=').slice(1).join('='), url: BASE }
]);
const mobile = await phone.newPage();
await mobile.goto(`${BASE}/settings`);
const mrow = mobile.locator('.anilist');
await mrow.scrollIntoViewIfNeeded();
const box = await mrow.boundingBox();
const overflow = await mobile.evaluate(
	() => document.documentElement.scrollWidth > document.documentElement.clientWidth
);
check('the row fits a phone', !!box && box.width <= 412 && !overflow, `${box?.width}px wide`);
await mobile.screenshot({ path: join(out, '3-phone-row.png') });

check('no console errors', errors.length === 0, errors.join(' | '));

await browser.close();
const failed = results.filter((r) => !r).length;
console.log(`\n${results.length - failed}/${results.length} checks passed`);
process.exit(failed ? 1 : 0);
