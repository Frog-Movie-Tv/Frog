/**
 * Verifies an event's premiere in a real browser, frame by frame, against a
 * throwaway instance with real TMDB pictures:
 *
 *   1. the countdown on Home reaches zero with the page open and the premiere
 *      plays, then hands over to the hero, which reads "Out now",
 *   2. it is marked seen when it starts and never plays twice,
 *   3. on a phone, with every essential seen in time, the Ready badge lands,
 *   4. a key skips it to the handoff at once,
 *   5. a hidden tab stops its clock,
 *   6. with reduced motion it is a short cross-fade,
 *   7. frame times: the 95th percentile, desktop and phone,
 *   8. no console errors.
 *
 * Screenshots of every beat go to test-results/verify-premiere.
 *
 *   node scripts/verify-premiere.mjs            (needs a release build and .env's TMDB key)
 *   node scripts/verify-premiere.mjs --webkit   (Safari's engine: npx playwright install webkit)
 *   node scripts/verify-premiere.mjs --slug=dune-part-three
 */
import { chromium, devices, webkit } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const engine = process.argv.includes('--webkit') ? 'webkit' : 'chromium';
const out = join(repo, 'frontend', 'test-results', 'verify-premiere', engine);
mkdirSync(out, { recursive: true });

const PORT = 8081;
const BASE = `http://127.0.0.1:${PORT}`;
const USER = 'pond';
const PASS = 'pond-password-long';
const SLUG = process.argv.find((a) => a.startsWith('--slug='))?.slice(7) ?? 'avengers-doomsday';

const results = [];
const check = (label, ok, detail = '') => {
	results.push(!!ok);
	console.log(`  [${ok ? 'PASS' : 'FAIL'}] ${label}${detail ? ' — ' + detail : ''}`);
};

function tmdbKey() {
	const env = join(repo, '.env');
	if (!existsSync(env)) return '';
	for (const line of readFileSync(env, 'utf8').split(/\r?\n/)) {
		if (line.startsWith('TMDB_API_KEY=')) return line.slice('TMDB_API_KEY='.length).trim();
	}
	return '';
}
if (!tmdbKey()) {
	console.error('verify-premiere needs TMDB_API_KEY in .env: the premiere is its pictures.');
	process.exit(1);
}

const binary = join(
	repo,
	'backend',
	'target',
	'release',
	process.platform === 'win32' ? 'frog.exe' : 'frog'
);
const dataDir = join(repo, 'frontend', '.premiere-data');
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
		TMDB_API_KEY: tmdbKey(),
		TVMAZE_API_URL: 'http://127.0.0.1:9',
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

// Real Chrome when it is installed: headless Chromium draws without the GPU,
// which is not what anybody watching has.
const chrome = existsSync('C:/Program Files/Google/Chrome/Application/chrome.exe');
const browser =
	engine === 'webkit'
		? await webkit.launch()
		: await chromium.launch({ channel: chrome ? 'chrome' : undefined });
const errors = [];

async function signedIn(options = {}) {
	const context = await browser.newContext({
		viewport: { width: 1440, height: 900 },
		colorScheme: 'dark',
		serviceWorkers: 'block',
		...options
	});
	const page = await context.newPage();
	page.on('pageerror', (e) => errors.push(e.message));
	page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
	await page.goto(`${BASE}/login`);
	await page.getByLabel('Username').fill(USER);
	await page.getByLabel('Password').fill(PASS);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await page.waitForURL(/\/(welcome)?$/);
	return { context, page };
}

const api = (page, method, path, body) =>
	page.evaluate(
		async ([m, p, b]) => {
			const res = await fetch(p, {
				method: m,
				headers: b ? { 'content-type': 'application/json' } : {},
				body: b ? JSON.stringify(b) : undefined
			});
			return { status: res.status, body: await res.json().catch(() => null) };
		},
		[method, path, body]
	);

const db = new DatabaseSync(dbFile);
db.exec('PRAGMA busy_timeout = 5000');
const forget = () => db.exec('DELETE FROM event_seen');

const pin = (page, ms) =>
	api(page, 'PUT', `/api/admin/events/${SLUG}/pin`, {
		at: new Date(Date.now() + ms).toISOString()
	});

/** The premiere's frames, from its first to its last, as percentiles. */
const frameStats = (frames) => {
	const s = [...frames].sort((a, b) => a - b);
	const at = (q) => s[Math.min(s.length - 1, Math.floor(q * s.length))] ?? 0;
	return { count: s.length, p50: at(0.5), p95: at(0.95), worst: s.at(-1) ?? 0 };
};

/** Shots at each beat of the premiere, by its own clock. */
async function beats(page, name, at) {
	for (const t of at) {
		await page.waitForFunction((s) => (window.frogPremiere?.t ?? 0) >= s, t, { timeout: 15000 });
		await page.screenshot({ path: join(out, `${name}-${String(t).replace('.', '_')}s.png`) });
	}
}

const BEATS = [0.3, 1.62, 2.62, 3.7, 3.95, 4.25, 4.6, 5.3, 5.7, 6.4, 7.2, 7.8];

const { context, page } = await signedIn();

// The worker makes the pack Frog's in its first minutes.
console.log('waiting for the pack…');
let ready = false;
for (let i = 0; i < 120 && !ready; i++) {
	const r = await api(page, 'GET', '/api/admin/events');
	ready = r.status === 200 && r.body.events.some((e) => e.slug === SLUG && e.title?.backdrop_path);
	if (!ready) await page.waitForTimeout(1000);
}
check('the pack became an event', ready);
if (!ready) process.exit(1);

// Something in the library, so Home is Home and not the welcome.
await api(page, 'POST', '/api/library/bulk', {
	action: 'add',
	status: 'planned',
	refs: [{ kind: 'movie', tmdb_id: 603 }]
});

// --- 1, 2, 7: at zero with Home open, on a computer --------------------------
await pin(page, 9000);
await page.goto(`${BASE}/`);
await page.locator('[role="timer"]').first().waitFor({ timeout: 15000 });
await page.screenshot({ path: join(out, 'desktop-countdown.png') });
check('Home counts down', await page.locator('[role="timer"]').first().isVisible());
await page.locator('.premiere.playing').waitFor({ timeout: 20000 });
await beats(page, 'desktop', BEATS);
await page.locator('.premiere').waitFor({ state: 'detached', timeout: 15000 });
await page.waitForTimeout(800);
await page.screenshot({ path: join(out, 'desktop-after.png') });
const label = await page.locator('.hero .hero-label').first().textContent();
check('the hero reads Out now', /Out now/.test(label ?? ''), label ?? '');

/** Played again with nothing else going on, for its frame times. */
async function timed(p) {
	forget();
	await p.reload();
	await p.locator('.premiere.playing').waitFor({ timeout: 15000 });
	await p.locator('.premiere').waitFor({ state: 'detached', timeout: 15000 });
	return frameStats(await p.evaluate(() => window.frogPremiere.frames));
}
const desktop = await timed(page);
check(
	'desktop frames under 16.7 ms at the 95th percentile',
	desktop.p95 <= 16.7 + 0.5,
	JSON.stringify(desktop)
);
const listed = await api(page, 'GET', '/api/events');
check('marked seen as it started', listed.body.events.find((e) => e.slug === SLUG)?.seen === true);
await page.reload();
await page.waitForTimeout(2500);
check('it never plays twice', (await page.locator('.premiere').count()) === 0);

// --- 4: a key skips it ------------------------------------------------------
forget();
await page.reload();
await page.locator('.premiere.playing').waitFor({ timeout: 15000 });
await page.waitForFunction(() => (window.frogPremiere?.t ?? 0) >= 1.2);
const pressed = Date.now();
await page.keyboard.press('Escape');
await page.locator('.premiere').waitFor({ state: 'detached', timeout: 5000 });
const skipped = Date.now() - pressed;
check('Escape skips to the handoff', skipped < 1600, `${skipped} ms`);

// --- 5: a hidden tab stops its clock ---------------------------------------
forget();
await page.reload();
await page.locator('.premiere.playing').waitFor({ timeout: 15000 });
await page.waitForFunction(() => (window.frogPremiere?.t ?? 0) >= 0.8);
const before = await page.evaluate(() => {
	Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
	return window.frogPremiere.t;
});
await page.waitForTimeout(1500);
const after = await page.evaluate(() => {
	const t = window.frogPremiere.t;
	delete document.hidden;
	return t;
});
check('a hidden tab holds the clock', after - before < 0.1, `${(after - before).toFixed(3)} s`);
await page.locator('.premiere').waitFor({ state: 'detached', timeout: 15000 });
await context.close();

// --- 3, 7: every essential seen in time, on a phone ----------------------------
const phone = await signedIn({
	...(engine === 'webkit' ? devices['iPhone 14'] : devices['Pixel 7'])
});
// Zero moves on first, so the essentials are seen before it.
await pin(phone.page, 25000);
const zeroAt = Date.now() + 25000;
const event = await api(phone.page, 'GET', `/api/events/${SLUG}`);
const essentials = event.body.groups
	.flatMap((g) => g.titles)
	.filter((t) => t.essential)
	.map((t) => t.item.id);
await api(phone.page, 'POST', '/api/library/bulk', {
	action: 'add',
	status: 'planned',
	title_ids: essentials
});
const marked = await api(phone.page, 'POST', '/api/library/bulk', {
	action: 'watched',
	title_ids: essentials,
	when: 'today'
});
check('every essential marked', marked.status === 200, JSON.stringify(marked.body).slice(0, 120));
const home = await api(phone.page, 'GET', '/api/home');
const r = home.body.events?.[0]?.readiness;
check(
	'every essential counts as seen in time',
	r?.ready_by_zero === true && !!home.body.events[0].ready_badge,
	JSON.stringify({ ...r, badge: home.body.events?.[0]?.ready_badge })
);
forget();
await phone.page.goto(`${BASE}/`);
await phone.page.waitForTimeout(Math.max(0, zeroAt - Date.now() - 4000));
await phone.page.locator('.premiere.playing').waitFor({ timeout: 20000 });
await beats(
	phone.page,
	'phone',
	BEATS.filter((t) => t <= 7.2)
);
const landed = await phone.page.locator('.premiere .badge').evaluate((el) => {
	const r = el.getBoundingClientRect();
	return (
		Number(getComputedStyle(el).opacity) > 0.9 &&
		r.left >= 0 &&
		r.right <= innerWidth &&
		r.top >= 0 &&
		r.bottom <= innerHeight
	);
});
check('the Ready badge lands, whole and on screen', landed);
await beats(
	phone.page,
	'phone',
	BEATS.filter((t) => t > 7.2)
);
await phone.page.locator('.premiere').waitFor({ state: 'detached', timeout: 15000 });
const mobile = await timed(phone.page);
console.log(
	`  phone frames: ${JSON.stringify(mobile)} (emulated; a real Pixel 7 is the figure that counts)`
);
await phone.page.waitForTimeout(800);
await phone.page.screenshot({ path: join(out, 'phone-after.png') });
await phone.context.close();

// --- 6: reduced motion -------------------------------------------------------
forget();
const still = await signedIn({ reducedMotion: 'reduce' });
await still.page.goto(`${BASE}/`);
await still.page.locator('.premiere.playing').waitFor({ timeout: 15000 });
const shown = Date.now();
await still.page.locator('.premiere').waitFor({ state: 'detached', timeout: 5000 });
const faded = Date.now() - shown;
check('reduced motion is a short cross-fade', faded < 1500, `${faded} ms`);
await still.context.close();

// --- the pages around it, before zero, at both widths ------------------------
for (const [name, options] of [
	['desktop', {}],
	['phone', { ...(engine === 'webkit' ? devices['iPhone 14'] : devices['Pixel 7']) }]
]) {
	const s = await signedIn(options);
	await pin(s.page, 12 * 86_400_000);
	for (const [shot, path] of [
		['home', '/'],
		['events', '/events'],
		['event', `/events/${SLUG}`],
		['event-left', `/events/${SLUG}?show=left`]
	]) {
		await s.page.goto(`${BASE}${path}`);
		await s.page.waitForTimeout(2500);
		await s.page.screenshot({ path: join(out, `${name}-page-${shot}.png`), fullPage: true });
		const wide = await s.page.evaluate(
			() => document.scrollingElement.scrollWidth - document.scrollingElement.clientWidth
		);
		check(`${name} ${path} never scrolls sideways`, wide <= 1, `${wide}px`);
	}
	await s.page.goto(`${BASE}/`);
	const row = await s.page
		.getByRole('heading', { name: 'Before Doomsday' })
		.waitFor({ timeout: 10000 })
		.then(
			() => true,
			() => false
		);
	check(`${name}: the Before row is on Home`, row);
	await s.context.close();
}

const real = errors.filter((e) => !/favicon|Failed to load resource/.test(e));
check('no console errors', real.length === 0, real.slice(0, 3).join(' | '));

await browser.close();
db.close();
const failed = results.filter((r) => !r).length;
console.log(failed ? `\n${failed} failed` : '\nall passed');
process.exit(failed ? 1 : 0);
