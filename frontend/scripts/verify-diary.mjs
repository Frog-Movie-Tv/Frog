/**
 * Walks the bulk-marking and diary flow in real Chrome at phone width and
 * saves screenshots, so a fix is seen working rather than assumed:
 *
 *   1. the season header fits a 375px card,
 *   2. marking a season asks when it was watched,
 *   3. "I don't know" keeps those episodes out of the diary,
 *   4. "Mark all watched" on one date becomes a single grouped diary entry,
 *   5. that group can be moved out of the diary afterwards.
 *
 * Needs TMDB_API_KEY in .env (the show's seasons come from TMDB).
 *   node scripts/verify-diary.mjs [outDir]
 */
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const out = resolve(process.argv[2] ?? join(repo, 'frontend', 'test-results', 'verify-diary'));
mkdirSync(out, { recursive: true });

const PORT = 8093;
const BASE = `http://127.0.0.1:${PORT}`;
const SHOW = 95396; // Severance

const key = (() => {
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

const binary = ['release', 'debug']
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

const chrome = existsSync('C:/Program Files/Google/Chrome/Application/chrome.exe');
const browser = await chromium.launch({ channel: chrome ? 'chrome' : undefined });
const context = await browser.newContext({
	viewport: { width: 375, height: 812 },
	deviceScaleFactor: 2,
	isMobile: true,
	hasTouch: true
});
const page = await context.newPage();
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));
page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));

await page.goto(`${BASE}/login`);
await page.getByLabel('Username').fill('verify');
await page.getByLabel('Password').fill('verify-password');
await page.getByRole('button', { name: 'Sign in' }).click();
await page.waitForURL(`${BASE}/`);

// Track the show so every season is local.
await page.request.put(`${BASE}/api/titles/tv/${SHOW}/library`, { data: { status: 'watching' } });

await page.goto(`${BASE}/title/tv/${SHOW}`);
await page.getByRole('heading', { name: 'Seasons' }).scrollIntoViewIfNeeded();
const action = page.getByRole('button', { name: 'Mark watched', exact: true }).first();
await action.waitFor();
await page.waitForTimeout(500);

console.log('\n== season header at 375px ==');
const card = page.locator('.season.open').first();
const [cardBox, actionBox] = await Promise.all([card.boundingBox(), action.boundingBox()]);
check(
	'the season action stays inside its card',
	actionBox.x >= cardBox.x && actionBox.x + actionBox.width <= cardBox.x + cardBox.width + 0.5,
	`card ${Math.round(cardBox.x)}–${Math.round(cardBox.x + cardBox.width)}, button ${Math.round(actionBox.x)}–${Math.round(actionBox.x + actionBox.width)}`
);
const meta = card.locator('.meta');
const metaBox = await meta.boundingBox();
const lineHeight = await meta.evaluate((el) => parseFloat(getComputedStyle(el).lineHeight) || 18);
check(
	'"episodes · year" sits on one line',
	metaBox.height <= lineHeight * 1.4,
	`${Math.round(metaBox.height)}px tall`
);
await card.screenshot({ path: join(out, '1-season-header-375.png') });

console.log('\n== marking a season asks when ==');
await action.click();
const dialog = page.getByRole('dialog', { name: 'When did you watch this?' });
await dialog.waitFor();
check('the "When did you watch this?" dialog opens', await dialog.isVisible());
for (const option of ['Today', 'On release dates', 'On a date', "I don't know"]) {
	check(
		`offers "${option}"`,
		(await dialog.getByRole('radio', { name: new RegExp(option) }).count()) === 1
	);
}
await page.screenshot({ path: join(out, '2-when-dialog-375.png') });
await dialog.getByRole('radio', { name: /I don't know/ }).click();
await dialog.getByRole('button', { name: 'Mark watched' }).click();
await dialog.waitFor({ state: 'hidden' });
await page.waitForTimeout(800);

console.log('\n== "I don\'t know" stays out of the diary ==');
await page.goto(`${BASE}/diary`);
await page.waitForTimeout(800);
check('the diary lists nothing', (await page.getByText('Nothing watched yet').count()) === 1);
const note = await page.getByText(/without a date are counted/).textContent();
check(
	'it says how many undated watches exist',
	/\d+ watches without a date/.test(note ?? ''),
	note?.trim()
);

console.log('\n== a whole show on one date is one diary entry ==');
await page.goto(`${BASE}/title/tv/${SHOW}`);
await page.getByRole('button', { name: 'Mark all watched' }).click();
await dialog.waitFor();
await dialog.getByRole('radio', { name: /On a date/ }).click();
await dialog.locator('input[type="date"]').fill('2026-10-24');
await dialog.getByRole('button', { name: 'Mark watched' }).click();
await dialog.waitFor({ state: 'hidden' });
await page.waitForTimeout(1200);

await page.goto(`${BASE}/diary`);
await page.waitForTimeout(800);
const groups = page.locator('.entry');
const groupCount = await groups.count();
const subtitle = await groups.first().locator('.sub').textContent();
// Season 1 is undated; only the other seasons land on the chosen date.
check('the day shows one entry for the show', groupCount === 1, `${groupCount} entries`);
check(
	'the entry reads as a range with a count',
	/S\d\dE\d\d–S?\d*E?\d\d · \d+ episodes/.test(subtitle ?? ''),
	subtitle?.trim()
);
await groups.first().locator('.what').click();
await page.waitForTimeout(300);
const expanded = await groups.first().locator('.episodes li').count();
check('it expands to its episodes', expanded > 1, `${expanded} episodes`);
await page.screenshot({ path: join(out, '3-diary-grouped-375.png'), fullPage: true });

console.log('\n== the group can be taken out of the diary ==');
await groups
	.first()
	.getByRole('button', { name: /Change date/ })
	.click();
await dialog.waitFor();
check(
	'"Today" is not offered when redating',
	(await dialog.getByRole('radio', { name: /^Today/ }).count()) === 0
);
await dialog.getByRole('radio', { name: /I don't know/ }).click();
await dialog.getByRole('button', { name: 'Mark watched' }).click();
await page.waitForTimeout(1200);
check('the diary is empty again', (await page.getByText('Nothing watched yet').count()) === 1);

const progress = await (await page.request.get(`${BASE}/api/titles/tv/${SHOW}`)).json();
check(
	'every episode still counts as watched',
	(progress.item.watched_episodes > 0 &&
		progress.item.watched_episodes === progress.item.total_episodes - (progress.next_up ? 1 : 0)) ||
		progress.item.watched_episodes > 15,
	`${progress.item.watched_episodes} of ${progress.item.total_episodes}`
);

console.log('\n== console ==');
check('no console errors', errors.length === 0, errors.slice(0, 3).join(' | '));

await browser.close();
const failed = results.filter((r) => !r).length;
console.log(`\nscreenshots in ${out}`);
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
// The 'exit' handler never runs while the server keeps Node alive.
server.kill();
process.exitCode = failed ? 1 : 0;
