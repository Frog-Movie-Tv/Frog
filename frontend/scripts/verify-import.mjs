/**
 * Walks the import review in real Chromium with a Netflix file, on desktop and
 * at phone width, and saves screenshots:
 *
 *   1. shows are grouped, and the chips count each kind of problem,
 *   2. a chip shows only its own rows, and bulk actions reach only those,
 *   3. keeping matches and leaving rows out can both be undone,
 *   4. a doubtful row is never touched by "Keep",
 *   5. nothing scrolls sideways on a phone,
 *   6. the import itself still finishes.
 *
 * Needs TMDB_API_KEY in .env. Takes a real export, or the fixture:
 *   node scripts/verify-import.mjs [file.csv] [outDir]
 */
import { chromium, devices } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const file = resolve(
	process.argv[2] ?? join(repo, 'frontend', 'tests', 'fixtures', 'netflix-viewing-history.csv')
);
const out = resolve(process.argv[3] ?? join(repo, 'frontend', 'test-results', 'verify-import'));
mkdirSync(out, { recursive: true });

const PORT = 8092;
const BASE = `http://127.0.0.1:${PORT}`;

const key = (() => {
	if (process.env.TMDB_API_KEY) return process.env.TMDB_API_KEY;
	const env = join(repo, '.env');
	if (!existsSync(env)) return '';
	const line = readFileSync(env, 'utf8')
		.split(/\r?\n/)
		.find((l) => l.startsWith('TMDB_API_KEY='));
	return line ? line.slice('TMDB_API_KEY='.length).trim() : '';
})();
if (!key) {
	console.error('TMDB_API_KEY is needed in .env');
	process.exit(1);
}

const binary =
	process.env.FROG_BINARY ??
	['release', 'debug']
		.map((p) =>
			join(repo, 'backend', 'target', p, process.platform === 'win32' ? 'frog.exe' : 'frog')
		)
		.find(existsSync);

const dataDir = join(repo, 'frontend', '.verify-data');
rmSync(dataDir, { recursive: true, force: true });
mkdirSync(dataDir, { recursive: true });

const server = spawn(binary, [], {
	stdio: 'ignore',
	env: {
		...process.env,
		DATABASE_URL: `sqlite:${join(dataDir, 'v.db').split('\\').join('/')}?mode=rwc`,
		FROG_BIND: `127.0.0.1:${PORT}`,
		FROG_STATIC_DIR: join(repo, 'frontend', 'build'),
		FROG_ADMIN_USERNAME: 'verify',
		FROG_ADMIN_PASSWORD: 'verify-password',
		TMDB_API_KEY: key,
		RUST_LOG: 'frog=warn'
	}
});
process.on('exit', () => {
	server.kill();
	try {
		rmSync(dataDir, { recursive: true, force: true });
	} catch {
		// Windows holds the database a moment after the kill; the next run clears it.
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

const results = [];
const check = (label, ok, detail = '') => {
	results.push(ok);
	console.log(`${ok ? 'ok  ' : 'FAIL'} ${label}${detail ? ` — ${detail}` : ''}`);
};

const browser = await chromium.launch();
const errors = [];

async function open(context) {
	const page = await context.newPage();
	page.on('pageerror', (e) => errors.push(e.message));
	page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
	await page.goto(`${BASE}/login`);
	await page.getByLabel(/username/i).fill('verify');
	await page.getByLabel(/password/i).fill('verify-password');
	await page.keyboard.press('Enter');
	await page.waitForURL((u) => !u.pathname.startsWith('/login'));
	await page.goto(`${BASE}/import`);
	await page.getByRole('button', { name: /^Netflix/ }).click();
	await page.locator('input[type=file]').setInputFiles(file);
	// The bar appears a moment after the file is read, and only then can it go.
	await page.locator('.progress').waitFor({ state: 'attached', timeout: 30_000 });
	await page.locator('.progress').waitFor({ state: 'detached', timeout: 300_000 });
	return page;
}

const chip = (page, name) => page.locator('.chips .chip', { hasText: name });
const count = async (page, name) =>
	Number((await chip(page, name).locator('.n').innerText()).trim());
const needLook = async (page) => Number(await page.locator('.counts .warn').innerText());
const visible = (page) => page.locator('.rows .row');

const desktop = await browser.newContext({ viewport: { width: 1280, height: 900 } });
const page = await open(desktop);

const chipNames = await page.locator('.chips .chip').allInnerTexts();
console.log(chipNames.map((c) => c.replace(/\s+/g, ' ')).join(' | '));
const total = await count(page, 'All');
const look = await needLook(page);
const shows = await page.locator('.show').count();
check('rows are grouped under their show', shows > 0, `${shows} shows of ${total} rows`);
await page.screenshot({ path: join(out, 'all-desktop.png') });

const closed = await page
	.locator('.show')
	.evaluateAll((all) => all.findIndex((s) => !s.querySelector('.row')));
if (closed >= 0) {
	const quiet = page.locator('.show').nth(closed);
	await quiet.locator('.show-toggle').click();
	check('a show without problems opens by hand', (await quiet.locator('.row').count()) > 0);
	await quiet.locator('.show-toggle').click();
}

await chip(page, 'Needs a look').click();
check('"Needs a look" shows that many rows', (await visible(page).count()) === look, `${look}`);
await page.screenshot({ path: join(out, 'look-desktop.png'), fullPage: true });

await chip(page, 'Closest match').click();
const closest = await count(page, 'Closest match');
check('a chip shows only its rows', (await visible(page).count()) === closest, `${closest}`);
await page.screenshot({ path: join(out, 'closest-desktop.png'), fullPage: true });

// One wrong row left out by hand must escape the bulk action.
const firstBox = visible(page).first().locator('input[type=checkbox]');
await firstBox.uncheck();
check(
	'leaving a row out takes it off the count',
	(await count(page, 'Closest match')) === closest - 1
);
check('it stays on screen, dimmed', (await page.locator('.rows .row.off').count()) === 1);

const keep = page.getByRole('button', { name: /^Keep (this match|these \d+ matches)/ });
check(
	'Keep counts only rows still included',
	(await keep.innerText()).includes(String(closest - 1)),
	(await keep.innerText()).trim()
);
await keep.click();
check('the rest are settled', (await needLook(page)) === look - closest, `${await needLook(page)}`);
await page.getByRole('button', { name: 'Undo' }).click();
check('Undo brings them back', (await needLook(page)) === look - 1);
await firstBox.check();
check('the row left out is back too', (await needLook(page)) === look);

await chip(page, 'Check the match').click();
const doubtful = await count(page, 'Check the match');
check(
	'Keep is not offered for doubtful rows',
	(await page.getByRole('button', { name: /^Keep/ }).count()) === 0,
	`${doubtful} rows`
);
await page.getByRole('button', { name: /^Leave (this one|these \d+) out/ }).click();
check('Leave out reaches only those rows', (await needLook(page)) === look - doubtful);
await page.getByRole('button', { name: 'Undo' }).click();
check('and comes back', (await needLook(page)) === look);

await chip(page, 'All').click();
const includes = await page.locator('.rows input[type=checkbox]').count();
check('All is back', includes > 0 && (await page.locator('.show').count()) === shows);

const button = page.getByRole('button', { name: /^Import \d+ title/ });
const ready = Number((await button.innerText()).match(/\d+/)[0]);
check('every matched row is ready', ready > 0, `${ready}`);

const phone = await browser.newContext({ ...devices['Pixel 7'] });
const small = await open(phone);
await small.screenshot({ path: join(out, 'all-phone.png') });
await chip(small, 'Needs a look').click();
await small.screenshot({ path: join(out, 'look-phone.png') });
const wide = await small.evaluate(
	() => document.documentElement.scrollWidth - document.documentElement.clientWidth
);
check('nothing scrolls sideways on a phone', wide <= 0, `${wide}px`);
await phone.close();

await button.click();
await page.getByText('Import finished').waitFor({ timeout: 300_000 });
const summary = (await page.locator('.done dl').innerText()).replace(/\s+/g, ' ');
check('the import finishes', !(await page.locator('dd.bad').count()), summary);

check('no browser errors', errors.length === 0, errors.slice(0, 5).join(' | '));
await browser.close();

const failed = results.filter((r) => !r).length;
console.log(`\n${results.length - failed} of ${results.length} passed`);
process.exit(failed ? 1 : 0);
