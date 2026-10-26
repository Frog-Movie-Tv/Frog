/**
 * Verifies the "Because you watched" rows on Home in real Chrome, against
 * real TMDB:
 *
 *   1. the highest rated title leads with its own row of picks,
 *   2. nothing already in the library is offered, and no pick repeats,
 *   3. a pick opens its title page,
 *   4. a Home layout saved before this row existed gains it once, and turning
 *      it off in Settings keeps it off.
 *
 *   node scripts/verify-because.mjs     (needs the TMDB key and a release build)
 */
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const out = join(repo, 'frontend', 'test-results', 'verify-because');
mkdirSync(out, { recursive: true });

const PORT = 8088;
const BASE = `http://127.0.0.1:${PORT}`;
const USER = 'because';
const PASS = 'because-password';
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
const dataDir = join(repo, 'frontend', '.because-data');
rmSync(dataDir, { recursive: true, force: true });
mkdirSync(dataDir, { recursive: true });
const server = spawn(binary, [], {
	stdio: 'ignore',
	env: {
		...process.env,
		DATABASE_URL: `sqlite:${join(dataDir, 'b.db').split('\\').join('/')}?mode=rwc`,
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
// Interstellar rated highest; The Dark Knight and Inception also loved, and
// both are things TMDB recommends for Interstellar, so they must not appear.
const library = [
	['movie', 157336, 100],
	['movie', 155, 90],
	['movie', 27205, 85],
	['tv', 1396, 90],
	['movie', 680, 40]
];
for (const [kind, id, rating] of library) {
	await call('PUT', `/api/titles/${kind}/${id}/library`, { status: 'completed', rating });
}
// A Home layout saved before this row existed.
await call('PUT', '/api/settings', {
	homeRows: ['continue', 'upnext', 'thisweek', 'recent', 'favourites']
});

const api = await call('GET', '/api/home/because');
check('there are rows', api.rows.length >= 2, `${api.rows.length} rows`);
check(
	'the best rated title leads',
	api.rows[0]?.seed.name === 'Interstellar',
	api.rows[0]?.seed.name
);
const tracked = new Set(library.map(([k, id]) => `${k}${id}`));
const all = api.rows.flatMap((r) => r.items.map((i) => `${i.kind}${i.tmdb_id}`));
check(
	'nothing in the library is offered',
	all.every((k) => !tracked.has(k)),
	all.filter((k) => tracked.has(k)).join(',')
);
check('no pick repeats across rows', new Set(all).size === all.length);
check('a title rated 4 is never a seed', !api.rows.some((r) => r.seed.tmdb_id === 680));

const chrome = existsSync('C:/Program Files/Google/Chrome/Application/chrome.exe');
const browser = await chromium.launch({ channel: chrome ? 'chrome' : undefined });
for (const [label, viewport, extra] of [
	['desktop', { width: 1280, height: 900 }, {}],
	['phone', { width: 375, height: 812 }, { isMobile: true, hasTouch: true }]
]) {
	console.log(`\n== ${label} ==`);
	const context = await browser.newContext({ viewport, ...extra });
	const page = await context.newPage();
	const errors = [];
	page.on('pageerror', (e) => errors.push(e.message));
	page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
	await page.goto(`${BASE}/login`);
	await page.getByLabel('Username').fill(USER);
	await page.getByLabel('Password').fill(PASS);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await page.waitForURL(`${BASE}/`);

	const heading = page.getByRole('heading', { name: 'Because you watched Interstellar' });
	await heading.waitFor({ timeout: 30000 }).catch(() => undefined);
	check('an older saved layout gains the row', (await heading.count()) === 1);
	const heroLabel = (await page.locator('.hero-label').textContent())?.trim();
	check(
		'with nothing in progress the hero says what it is',
		heroLabel === 'Recently added',
		heroLabel
	);
	const rail = page.locator('section', { has: heading });
	const links = await rail
		.locator('a[href^="/title/"]')
		.evaluateAll((as) => as.map((a) => a.getAttribute('href')));
	check('the row shows the picks', links.length >= 4, `${links.length} posters`);
	check(
		'the row offers nothing from the library',
		links.every((h) => !library.some(([k, id]) => h === `/title/${k}/${id}`))
	);
	const overflow = await page.evaluate(
		() => document.documentElement.scrollWidth - window.innerWidth
	);
	check('Home fits the viewport', overflow <= 0, `${overflow}px too wide`);
	await heading.scrollIntoViewIfNeeded();
	await page.screenshot({ path: join(out, `1-home-${label}.png`) });

	if (label === 'desktop') {
		await rail.locator('a[href^="/title/"]').first().click();
		await page.waitForURL(/\/title\//);
		await page.locator('h1, .logo').first().waitFor();
		check('a pick opens its title page', true, new URL(page.url()).pathname);

		const seen = (await call('GET', '/api/settings')).homeRowsSeen ?? [];
		check('the layout remembers it was offered the row', seen.includes('because'), seen.join(','));
		await call('PUT', '/api/settings', {
			...(await call('GET', '/api/settings')),
			homeRows: ['continue', 'upnext', 'thisweek', 'recent', 'favourites']
		});
		await page.goto(`${BASE}/`);
		await page.getByRole('heading', { name: 'Recently added' }).waitFor();
		await page.waitForTimeout(1500);
		check(
			'turned off, it stays off',
			(await page.getByRole('heading', { name: /Because you watched/ }).count()) === 0
		);
		// Put it back for the phone run.
		await call('PUT', '/api/settings', {
			...(await call('GET', '/api/settings')),
			homeRows: ['continue', 'upnext', 'thisweek', 'because', 'recent', 'favourites']
		});
	}
	check('no errors in the console', errors.length === 0, errors.join(' | '));
	await context.close();
}

await browser.close();
server.kill();
const failed = results.filter((r) => !r).length;
console.log(
	`\nscreenshots in ${out}\n${failed ? failed + ' check(s) failed' : 'all checks passed'}`
);
process.exitCode = failed ? 1 : 0;
