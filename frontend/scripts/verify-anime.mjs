/**
 * Verifies anime imports end to end in real Chrome, against real TMDB:
 *
 *   1. a MyAnimeList XML export parses into season-aware rows,
 *   2. "Attack on Titan Season 2" matches TMDB's "Attack on Titan", season 2,
 *   3. importing ticks the right episodes of the right seasons, undated.
 *
 *   node scripts/verify-anime.mjs
 */
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const out = join(repo, 'frontend', 'test-results', 'verify-anime');
mkdirSync(out, { recursive: true });

const PORT = 8091;
const BASE = `http://127.0.0.1:${PORT}`;
const key = readFileSync(join(repo, '.env'), 'utf8')
	.split(/\r?\n/)
	.find((l) => l.startsWith('TMDB_API_KEY='))
	?.slice('TMDB_API_KEY='.length)
	.trim();

const results = [];
const check = (label, ok, detail = '') => {
	results.push(ok);
	console.log(`  [${ok ? 'PASS' : 'FAIL'}] ${label}${detail ? ' — ' + detail : ''}`);
};

const binary = join(
	repo,
	'backend',
	'target',
	'release',
	process.platform === 'win32' ? 'frog.exe' : 'frog'
);
const dataDir = join(repo, 'frontend', '.anime-data');
rmSync(dataDir, { recursive: true, force: true });
mkdirSync(dataDir, { recursive: true });

const xml = `<?xml version="1.0" encoding="UTF-8"?>
<myanimelist>
  <anime><series_title>Attack on Titan</series_title><series_type>TV</series_type><series_episodes>25</series_episodes>
    <my_watched_episodes>25</my_watched_episodes><my_score>9</my_score><my_status>Completed</my_status><my_finish_date>2014-01-01</my_finish_date></anime>
  <anime><series_title>Attack on Titan Season 2</series_title><series_type>TV</series_type><series_episodes>12</series_episodes>
    <my_watched_episodes>7</my_watched_episodes><my_score>0</my_score><my_status>Watching</my_status><my_finish_date>0000-00-00</my_finish_date></anime>
  <anime><series_title>Shingeki no Kyojin: The Final Season</series_title><series_type>TV</series_type><series_episodes>16</series_episodes>
    <my_watched_episodes>0</my_watched_episodes><my_score>0</my_score><my_status>Plan to Watch</my_status><my_finish_date>0000-00-00</my_finish_date></anime>
  <anime><series_title>Your Name.</series_title><series_type>Movie</series_type><series_episodes>1</series_episodes>
    <my_watched_episodes>1</my_watched_episodes><my_score>9</my_score><my_status>Completed</my_status><my_finish_date>2017-03-04</my_finish_date></anime>
</myanimelist>`;
const xmlPath = join(dataDir, 'animelist_frog.xml');
writeFileSync(xmlPath, xml);

const server = spawn(binary, [], {
	stdio: 'ignore',
	env: {
		...process.env,
		DATABASE_URL: `sqlite:${join(dataDir, 'a.db').split('\\').join('/')}?mode=rwc`,
		FROG_BIND: `127.0.0.1:${PORT}`,
		FROG_STATIC_DIR: join(repo, 'frontend', 'build'),
		FROG_ADMIN_USERNAME: 'anime',
		FROG_ADMIN_PASSWORD: 'anime-password',
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

const chrome = existsSync('C:/Program Files/Google/Chrome/Application/chrome.exe');
const browser = await chromium.launch({ channel: chrome ? 'chrome' : undefined });
const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
const page = await context.newPage();
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));

await page.goto(`${BASE}/login`);
await page.getByLabel('Username').fill('anime');
await page.getByLabel('Password').fill('anime-password');
await page.getByRole('button', { name: 'Sign in' }).click();
await page.waitForURL(`${BASE}/`);

console.log('\n== MyAnimeList export ==');
await page.goto(`${BASE}/import`);
await page.getByRole('button', { name: 'MyAnimeList', exact: true }).click();
await page.locator('input[type="file"]').setInputFiles(xmlPath);
await page.getByText(/ready/).first().waitFor({ timeout: 60000 });
await page.waitForFunction(() => !document.querySelector('.progress'), null, { timeout: 120000 });

const rowCount = await page.locator('ul.rows > li').count();
check('four entries read', rowCount === 4, `${rowCount} rows`);

const s2 = page.locator('ul.rows > li').filter({ hasText: 'Attack on Titan Season 2' });
const s2Match = (await s2.locator('.line strong').textContent())?.trim();
const s2Season = await s2.locator('.season input').inputValue();
check('Season 2 matches the TMDB show', s2Match === 'Attack on Titan', s2Match);
check('...as its season 2', s2Season === '2', `season ${s2Season}`);

const final = page.locator('ul.rows > li').filter({ hasText: 'The Final Season' });
check(
	'"The Final Season" asks which season it is',
	(await final.getByText('Choose which season this is').count()) === 1
);
await final.locator('.season input').fill('4');
await final.locator('.season input').dispatchEvent('change');

const film = page.locator('ul.rows > li').filter({ hasText: 'Your Name.' });
check(
	'the film is matched as a film',
	(await film.locator('.pill').textContent())?.trim() === 'Film'
);
await page.screenshot({ path: join(out, '1-mal-review.png'), fullPage: true });

await page.getByRole('button', { name: /^Import \d+/ }).click();
await page.getByRole('heading', { name: 'Import finished' }).waitFor({ timeout: 180000 });
await page.screenshot({ path: join(out, '2-mal-done.png') });

const search = await (await page.request.get(`${BASE}/api/library?q=Attack`)).json();
const aot = search.items.find((i) => i.name === 'Attack on Titan');
check('Attack on Titan is in the library', !!aot);
if (aot) {
	check('25 + 7 episodes ticked', aot.watched_episodes === 32, `${aot.watched_episodes} watched`);
	check(
		'still Watching, not sent back to the watchlist',
		aot.entry?.status === 'watching',
		aot.entry?.status
	);
}
const diary = await (await page.request.get(`${BASE}/api/diary`)).json();
const filmEntries = diary.entries.filter((e) => e.film).length;
check(
	'ticked episodes stay out of the diary; the film is in it',
	diary.entries.length === filmEntries && filmEntries === 1,
	`${diary.entries.length} entries, ${diary.undated} undated`
);

check('no page errors', errors.length === 0, errors.slice(0, 2).join(' | '));
await browser.close();
const failed = results.filter((r) => !r).length;
console.log(
	`\nscreenshots in ${out}\n${failed ? failed + ' check(s) failed' : 'all checks passed'}`
);
// The 'exit' handler never runs while the server keeps Node alive.
server.kill();
process.exitCode = failed ? 1 : 0;
