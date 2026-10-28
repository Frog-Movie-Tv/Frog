/**
 * Verifies the profile and stats layout in real Chrome, with no TMDB key: the
 * library is written straight into the throwaway database instead.
 *
 *   1. the profile picture is drawn in front of the banner, on the profile
 *      and in Settings, with and without a picture of its own,
 *   2. actors, directors and studios share one row on Stats, and stack on a
 *      phone,
 *   3. every mascot can be chosen and becomes the site icon,
 *   4. no console errors.
 *
 *   node scripts/verify-profile-stats.mjs     (needs a release build)
 */
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const out = join(repo, 'frontend', 'test-results', 'verify-profile-stats');
mkdirSync(out, { recursive: true });

const PORT = 8090;
const BASE = `http://127.0.0.1:${PORT}`;
const USER = 'pond';
const PASS = 'pond-password-long';

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
const dataDir = join(repo, 'frontend', '.profile-stats-data');
rmSync(dataDir, { recursive: true, force: true });
mkdirSync(dataDir, { recursive: true });
const dbFile = join(dataDir, 'p.db');
const server = spawn(binary, [], {
	stdio: 'ignore',
	env: {
		...process.env,
		DATABASE_URL: `sqlite:${dbFile.split('\\').join('/')}?mode=rwc`,
		FROG_BIND: `127.0.0.1:${PORT}`,
		FROG_STATIC_DIR: join(repo, 'frontend', 'build'),
		FROG_ADMIN_USERNAME: USER,
		FROG_ADMIN_PASSWORD: PASS,
		TMDB_API_KEY: '',
		RUST_LOG: 'frog=warn'
	}
});
const stop = () => {
	server.kill();
	try {
		rmSync(dataDir, { recursive: true, force: true });
	} catch {
		/* Windows may still hold the database; the next run clears it */
	}
};
process.on('exit', stop);
for (let i = 0; i < 80; i++) {
	try {
		if ((await fetch(`${BASE}/api/health`)).ok) break;
	} catch {
		/* starting */
	}
	await new Promise((r) => setTimeout(r, 250));
}

// --- a small library, written directly --------------------------------------
const db = new DatabaseSync(dbFile);
db.exec('PRAGMA busy_timeout = 5000');
const now = new Date().toISOString();
const addTitle = db.prepare(
	`INSERT INTO title (kind, tmdb_id, name, release_date, runtime, genres_json, refreshed_at, people_synced_at)
	 VALUES ('movie', ?, ?, '2020-01-01', 100, '["Drama"]', ?, ?)`
);
const addPerson = db.prepare(
	`INSERT INTO title_person (title_id, role, tmdb_id, name, ord) VALUES (?, ?, ?, ?, ?)`
);
const addEntry = db.prepare(
	`INSERT INTO library_entry (user_id, title_id, status, rating, added_at, updated_at)
	 VALUES (1, ?, 'completed', 80, ?, ?)`
);
const addWatch = db.prepare(
	`INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch, source)
	 VALUES (1, ?, NULL, ?, 0, 'manual')`
);
for (let i = 1; i <= 3; i++) {
	const { lastInsertRowid: id } = addTitle.run(9000 + i, `Pond Film ${i}`, now, now);
	addPerson.run(id, 'cast', 100 + i, `Actor ${i}`, 0);
	addPerson.run(id, 'director', 200 + i, `Director ${i}`, 0);
	addPerson.run(id, 'studio', 300 + i, `Studio ${i}`, 0);
	addEntry.run(id, now, now);
	addWatch.run(id, now);
}
db.close();

const chrome = existsSync('C:/Program Files/Google/Chrome/Application/chrome.exe');
const browser = await chromium.launch({ channel: chrome ? 'chrome' : undefined });
const errors = [];
async function signedIn(viewport, extra = {}) {
	const context = await browser.newContext({ viewport, ...extra });
	const page = await context.newPage();
	page.on('pageerror', (e) => errors.push(e.message));
	page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
	await page.goto(`${BASE}/login`);
	await page.getByLabel('Username').fill(USER);
	await page.getByLabel('Password').fill(PASS);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await page.waitForURL(/\/(welcome)?$/);
	return page;
}
const page = await signedIn({ width: 1280, height: 900 });
const put = (path, body) =>
	page.evaluate(
		async ([p, b]) => {
			const res = await fetch(p, {
				method: 'PUT',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify(b)
			});
			return res.status;
		},
		[path, body]
	);
await put('/api/settings', { onboarded: true });

/** Whatever is drawn near the top of `selector` belongs to it. */
const onTop = (p, selector) =>
	p.evaluate((sel) => {
		const el = document.querySelector(sel);
		if (!el) return false;
		const r = el.getBoundingClientRect();
		const hit = document.elementFromPoint(r.left + r.width / 2, r.top + 10);
		return !!hit && el.contains(hit);
	}, selector);

/** A picture drawn here and uploaded as `kind`. */
async function upload(kind, colour) {
	return page.evaluate(
		async ([k, c]) => {
			const canvas = document.createElement('canvas');
			canvas.width = k === 'banner' ? 1200 : 300;
			canvas.height = k === 'banner' ? 400 : 300;
			const g = canvas.getContext('2d');
			g.fillStyle = c;
			g.fillRect(0, 0, canvas.width, canvas.height);
			const blob = await new Promise((r) => canvas.toBlob(r, 'image/png'));
			const res = await fetch(`/api/account/images/${k}`, {
				method: 'PUT',
				headers: { 'content-type': 'image/png' },
				body: blob
			});
			return res.status;
		},
		[kind, colour]
	);
}

console.log('== Profile picture over the banner ==');
check('a banner picture uploads', (await upload('banner', '#e4572e')) === 200);
await page.goto(`${BASE}/people/${USER}`);
await page.locator('.banner img').waitFor();
check('without a picture, the initial is in front', await onTop(page, '.hero .avatar'));
await page.goto(`${BASE}/settings#account`);
await page.locator('.profile .banner img').waitFor();
check('and in Settings', await onTop(page, '.profile .picture .avatar'));
check('a profile picture uploads', (await upload('avatar', '#35b7c9')) === 200);
await page.goto(`${BASE}/people/${USER}`);
await page.locator('.hero .avatar img').waitFor();
check('with a picture, the picture is in front', await onTop(page, '.hero .avatar'));
await page.screenshot({ path: join(out, '1-profile.png') });
await page.goto(`${BASE}/settings#account`);
await page.locator('.profile .picture .avatar img').waitFor();
check('and in Settings', await onTop(page, '.profile .picture .avatar'));
await page.screenshot({ path: join(out, '2-settings-profile.png') });

console.log('\n== People on Stats ==');
await page.goto(`${BASE}/stats`);
await page.getByRole('heading', { name: 'Actors' }).waitFor();
const tops = await page.$$eval('.people-row > section', (els) =>
	els.map((e) => [e.querySelector('h2')?.textContent, Math.round(e.getBoundingClientRect().top)])
);
check(
	'actors, directors and studios in one row',
	tops.length === 3 && new Set(tops.map((t) => t[1])).size === 1,
	JSON.stringify(tops)
);
await page.locator('.people-row').screenshot({ path: join(out, '3-stats-people.png') });

console.log('\n== Mascots ==');
await page.goto(`${BASE}/settings#appearance`);
await page.getByRole('heading', { name: 'Mascot', level: 2 }).waitFor();
const tiles = await page.locator('.mascots .tile').count();
check('eight mascots to choose from', tiles === 8, `${tiles}`);
for (const [name, id] of [
	['Starry-eyed', 'starry'],
	['Bow', 'bow'],
	['Delighted', 'delighted'],
	['Sprout', 'sprout']
]) {
	await page.getByRole('button', { name, exact: true }).click();
	await page.waitForFunction(
		(want) => document.querySelector('link[rel="icon"]')?.getAttribute('href') === want,
		`/mascots/${id}.svg`
	);
	const svg = await fetch(`${BASE}/mascots/${id}.svg`);
	const manifest = await fetch(`${BASE}/manifest-${id}.webmanifest`);
	check(`${name} becomes the icon`, svg.ok && manifest.ok, `${svg.status} ${manifest.status}`);
}
await page.locator('.mascots').screenshot({ path: join(out, '4-mascots.png') });
await page.getByRole('button', { name: 'Classic', exact: true }).click();

console.log('\n== Phone ==');
const phone = await signedIn(
	{ width: 412, height: 915 },
	{ deviceScaleFactor: 2.6, isMobile: true, hasTouch: true }
);
await phone.goto(`${BASE}/stats`);
await phone.getByRole('heading', { name: 'Actors' }).waitFor();
const stacked = await phone.$$eval('.people-row > section', (els) =>
	els.map((e) => Math.round(e.getBoundingClientRect().top))
);
check('on a phone they stack', new Set(stacked).size === stacked.length, stacked.join(', '));
const wide = await phone.evaluate(
	() => document.documentElement.scrollWidth > document.documentElement.clientWidth
);
check('and fit the screen', !wide);
await phone.goto(`${BASE}/people/${USER}`);
await phone.locator('.hero .avatar img').waitFor();
check('the picture is in front on a phone too', await onTop(phone, '.hero .avatar'));
await phone.screenshot({ path: join(out, '5-phone-profile.png') });

check('no console errors', errors.length === 0, errors.slice(0, 3).join(' | '));

await browser.close();
const failed = results.filter((r) => !r).length;
console.log(`\n${results.length - failed}/${results.length} checks passed; screenshots in ${out}`);
stop();
process.exit(failed ? 1 : 0);
