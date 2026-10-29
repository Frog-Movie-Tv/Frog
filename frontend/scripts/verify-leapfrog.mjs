/**
 * Verifies the Leapfrog release in real Chrome, against real TMDB, with a
 * stand-in AniList (AniList's sign-in cannot be automated, and the list has to
 * change between syncs):
 *
 *   1. Settings is split into groups, each with its own address, on a laptop
 *      and on a phone,
 *   2. AniList connects through its sign-in and comes back to Settings, the
 *      first sync places a later season inside TMDB's numbering, an entry it
 *      cannot place waits for a choice, and a tick in Frog reaches AniList,
 *   3. a profile takes a name, an about text, a picture and a pinned title,
 *      and shows them,
 *   4. an administrator deletes an account, with a moment to take it back,
 *   5. no console errors anywhere.
 *
 *   node scripts/verify-leapfrog.mjs     (needs the TMDB key and a release build)
 */
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const out = join(repo, 'frontend', 'test-results', 'verify-leapfrog');
mkdirSync(out, { recursive: true });

const PORT = 8088;
const MOCK = 8086;
const BASE = `http://127.0.0.1:${PORT}`;
const ANILIST = `http://127.0.0.1:${MOCK}`;
const USER = 'leapfrog';
const PASS = 'leapfrog-password';
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

// --- a stand-in AniList ------------------------------------------------------
const entry = (id, name, episodes, status, progress, score = 0, format = 'TV') => ({
	status,
	score,
	progress,
	media: { id, format, episodes, seasonYear: 2020, title: { english: name, romaji: name } }
});
const fake = {
	entries: [
		entry(113415, 'JUJUTSU KAISEN', 24, 'COMPLETED', 24, 80),
		entry(145064, 'JUJUTSU KAISEN Season 2', 23, 'CURRENT', 5),
		entry(999001, 'Qwzx Nothing By This Name', 12, 'CURRENT', 3)
	],
	saves: []
};
const animeIds = {
	1: { anilist_id: 113415, tvdb_id: 377543, tvdb_season: 1, tvdb_epoffset: 0 },
	2: { anilist_id: 145064, tvdb_id: 377543, tvdb_season: 2, tvdb_epoffset: 0 }
};
const mock = createServer((req, res) => {
	const url = new URL(req.url, ANILIST);
	let body = '';
	req.on('data', (c) => (body += c));
	req.on('end', () => {
		const json = (v) => {
			res.writeHead(200, { 'content-type': 'application/json' });
			res.end(JSON.stringify(v));
		};
		if (url.pathname === '/api/v2/oauth/authorize') {
			// AniList's sign-in page, approved at once.
			const back = `${url.searchParams.get('redirect_uri')}?code=granted&state=${url.searchParams.get('state')}`;
			res.writeHead(302, { location: back });
			return res.end();
		}
		if (url.pathname === '/api/v2/oauth/token') {
			return json({ access_token: 'stand-in-token', expires_in: 31536000 });
		}
		if (url.pathname === '/anime_ids.json') return json(animeIds);
		if (url.pathname === '/anime_lists.json') return json([]);
		const { query = '', variables = {} } = body ? JSON.parse(body) : {};
		if (query.includes('Viewer'))
			return json({ data: { Viewer: { id: 77, name: 'pondwatcher' } } });
		if (query.includes('SaveMediaListEntry')) {
			fake.saves.push(variables);
			return json({
				data: {
					SaveMediaListEntry: {
						status: variables.status,
						progress: variables.progress,
						score: variables.scoreRaw
					}
				}
			});
		}
		if (query.includes('Page')) {
			return json({
				data: {
					Page: {
						media: [
							{
								id: 113415,
								format: 'TV',
								episodes: 24,
								title: { english: 'JUJUTSU KAISEN', romaji: 'Jujutsu Kaisen' }
							}
						]
					}
				}
			});
		}
		json({ data: { MediaListCollection: { lists: [{ entries: fake.entries }] } } });
	});
});
await new Promise((r) => mock.listen(MOCK, '127.0.0.1', r));

// --- Frog --------------------------------------------------------------------
const binary = join(
	repo,
	'backend',
	'target',
	'release',
	process.platform === 'win32' ? 'frog.exe' : 'frog'
);
const dataDir = join(repo, 'frontend', '.leapfrog-data');
rmSync(dataDir, { recursive: true, force: true });
mkdirSync(dataDir, { recursive: true });
const server = spawn(binary, [], {
	stdio: 'ignore',
	env: {
		...process.env,
		DATABASE_URL: `sqlite:${join(dataDir, 'l.db').split('\\').join('/')}?mode=rwc`,
		FROG_BIND: `127.0.0.1:${PORT}`,
		FROG_STATIC_DIR: join(repo, 'frontend', 'build'),
		FROG_ADMIN_USERNAME: USER,
		FROG_ADMIN_PASSWORD: PASS,
		TMDB_API_KEY: key,
		ANILIST_CLIENT_ID: '55123',
		ANILIST_CLIENT_SECRET: 'stand-in',
		ANILIST_REDIRECT_URI: `${BASE}/api/anilist/callback`,
		ANILIST_API_URL: `${ANILIST}/graphql`,
		ANILIST_SITE_URL: ANILIST,
		ANIME_IDS_URL: `${ANILIST}/anime_ids.json`,
		ANIME_LISTS_URL: `${ANILIST}/anime_lists.json`,
		RUST_LOG: 'frog=warn'
	}
});
const stop = () => {
	server.kill();
	mock.close();
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
await call('PUT', '/api/settings', { onboarded: true });

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
	await page.waitForURL(`${BASE}/`);
	return page;
}
const page = await signedIn({ width: 1280, height: 900 });
const toast = (text) => page.locator('.toast').filter({ hasText: text }).first();

console.log('== Settings groups ==');
await page.goto(`${BASE}/settings`);
await page.getByRole('heading', { name: 'Settings', level: 1 }).waitFor();
await page.getByRole('heading', { name: 'Account', level: 2 }).waitFor();
const tabs = page.getByRole('navigation', { name: 'Settings' }).getByRole('link');
check('six groups', (await tabs.count()) === 6, `${await tabs.count()}`);
check(
	'Account opens first, with the profile',
	(await page.getByRole('heading', { name: 'Profile', level: 2 }).count()) === 1
);
await tabs.filter({ hasText: 'Appearance' }).click();
await page.getByRole('heading', { name: 'Theme', level: 2 }).waitFor();
check('a group has its own address', page.url().endsWith('#appearance'), page.url());
check(
	'and only its own sections',
	(await page.getByRole('heading', { name: 'Profile', level: 2 }).count()) === 0
);
await page.goto(`${BASE}/settings#this-instance`);
check(
	'an old link still lands in the right group',
	(await page.locator('#this-instance').count()) === 1
);
await page.screenshot({ path: join(out, '1-settings-appearance.png') });

console.log('\n== AniList ==');
await page.goto(`${BASE}/settings#connections`);
await page.getByRole('button', { name: 'Connect AniList' }).click();
await page.waitForURL(/\/settings/);
await toast('AniList connected').waitFor({ timeout: 15000 });
check('signed in through AniList and came back', page.url().endsWith('#connections'), page.url());
await page.waitForTimeout(1500);
check(
	'it says so once, and tidies the address',
	(await page.locator('.toast').filter({ hasText: 'AniList connected' }).count()) === 1 &&
		!page.url().includes('anilist='),
	page.url()
);
await page.getByText('Connected as pondwatcher').waitFor({ timeout: 120000 });
const show = await call('GET', '/api/library?q=Jujutsu');
const jjk = show.items.find((i) => /jujutsu/i.test(i.name));
check('Jujutsu Kaisen is in the library', !!jjk);
check(
	'24 + 5 episodes, season 2 placed by id',
	jjk?.watched_episodes === 29,
	`${jjk?.watched_episodes}`
);
check('watching, from its latest season', jjk?.entry?.status === 'watching', jjk?.entry?.status);
const unknown = (await call('GET', '/api/library?q=Qwzx')).items;
check(
	'the unknown entry is kept as AniList has it, matched to nothing on TMDB',
	unknown.length === 1 && unknown[0].tmdb_id < 0,
	JSON.stringify(unknown.map((i) => [i.name, i.tmdb_id]))
);
console.log(
	'    entries:',
	JSON.stringify((await call('GET', '/api/anilist/entries')).entries.map((e) => [e.name, e.state]))
);
console.log('    status:', JSON.stringify(await call('GET', '/api/anilist')));
console.log('    url:', page.url());
await page.reload();
await page.screenshot({ path: join(out, '2a-before-unsure.png'), fullPage: true });
// Unmatched entries wait out of the way, in a group that says why.
const notFound = page.getByRole('tab', { name: /^Not found/ });
await notFound.waitFor({ timeout: 20000 });
check('the unknown entry waits, out of the way', true);
await notFound.click();
await page.screenshot({ path: join(out, '2-connections.png'), fullPage: true });

await page.getByRole('button', { name: 'Match', exact: true }).click();
await page.getByRole('button', { name: 'Not in Frog' }).click();
await notFound.waitFor({ state: 'detached', timeout: 10000 });
await page.getByRole('tab', { name: /^Not in Frog/ }).waitFor({ timeout: 10000 });
check('an entry can be left out of Frog', true);

// A tick in Frog travels to AniList, on the season it belongs to.
// Season 2's sixth episode, wherever TMDB numbers it.
const placed = (await call('GET', '/api/anilist/entries')).entries.find(
	(e) => e.media_id === 145064
);
check(
	'season 2 has a place in TMDB numbering',
	!!placed?.title,
	placed?.title ? `S${placed.title.season} from E${placed.title.from_episode}` : 'none'
);
const s = placed.title.season;
const n = placed.title.from_episode + 5;
await call('POST', `/api/titles/tv/${jjk.tmdb_id}/episodes/${s}/${n}/watch`, {});
await page.getByRole('button', { name: 'Sync now' }).click();
for (let i = 0; i < 60 && !fake.saves.length; i++) await new Promise((r) => setTimeout(r, 500));
check(
	'a tick in Frog reached AniList on the right entry',
	fake.saves.some((v) => v.mediaId === 145064 && v.progress === 6 && v.scoreRaw === 0),
	JSON.stringify(fake.saves)
);

console.log('\n== Profile ==');
await page.goto(`${BASE}/settings#account`);
await page.getByLabel('Name').fill('Leap Frog');
await page.getByLabel('Name').press('Tab');
await toast('Name saved').waitFor();
await page.getByLabel(/About you/).fill('Anime on weeknights, films on Sundays.');
await page.getByRole('button', { name: 'Save about' }).click();
await toast('About saved').waitFor();
// A real picture, drawn here and handed to the file input.
const png = await page.evaluate(async () => {
	const c = document.createElement('canvas');
	c.width = c.height = 300;
	const g = c.getContext('2d');
	g.fillStyle = '#6cc46b';
	g.fillRect(0, 0, 300, 300);
	g.fillStyle = '#07120e';
	g.beginPath();
	g.arc(150, 150, 90, 0, Math.PI * 2);
	g.fill();
	const blob = await new Promise((r) => c.toBlob(r, 'image/png'));
	return Array.from(new Uint8Array(await blob.arrayBuffer()));
});
await page
	.locator('.picture input[type=file]')
	.setInputFiles({ name: 'me.png', mimeType: 'image/png', buffer: Buffer.from(png) });
// A new picture is framed first: drag and zoom inside the circle, then use it.
await page.getByRole('button', { name: 'Use this picture' }).click();
await toast('Profile picture updated').waitFor({ timeout: 15000 });
await page.getByRole('button', { name: 'Choose', exact: true }).click();
await page.locator('.choice').first().click();
await page.waitForTimeout(800);
await page.getByLabel('Show my profile').check();
await page.waitForTimeout(500);
check('the top bar shows the picture', (await page.locator('.account .avatar img').count()) === 1);

await page.goto(`${BASE}/people/${USER}`);
await page.getByRole('heading', { name: 'Leap Frog', level: 1 }).waitFor();
check('the profile shows the name', true);
check(
	'and the about text',
	(await page.getByText('Anime on weeknights, films on Sundays.').count()) === 1
);
check('and the picture', (await page.locator('.hero .avatar img').count()) === 1);
check('and the pinned title', (await page.locator('.pins a, .pins .poster').count()) >= 1);
check('and the totals', (await page.getByText('Time watched').count()) === 1);
await page.screenshot({ path: join(out, '3-profile.png'), fullPage: true });

console.log('\n== 0.6.1 ==');
// A banner picture behind the profile picture: the picture must be on top.
const backdrop = (await call('GET', `/api/titles/tv/${jjk.tmdb_id}`)).item.backdrop_path;
await call('PUT', '/api/account/profile', { banner_backdrop: backdrop ?? '' });
/** Whatever is drawn at the middle of `selector` belongs to it. */
const onTop = (selector) =>
	page.evaluate((sel) => {
		const el = document.querySelector(sel);
		if (!el) return false;
		const r = el.getBoundingClientRect();
		// Near the top edge, where the picture overlaps the banner.
		const hit = document.elementFromPoint(r.left + r.width / 2, r.top + 12);
		return !!hit && el.contains(hit);
	}, selector);
await page.goto(`${BASE}/people/${USER}`);
await page.locator('.banner img').waitFor();
await page.locator('.hero .avatar img').waitFor();
check('a banner picture is set', !!backdrop);
check('the profile picture is in front of the banner', await onTop('.hero .avatar'));
await page.screenshot({ path: join(out, '3b-profile-banner.png') });
await page.goto(`${BASE}/settings#account`);
await page.locator('.profile .banner img').waitFor();
check('and in Settings too', await onTop('.profile .picture .avatar'));

// The people on Stats share one row.
await page.goto(`${BASE}/stats`);
await page.getByRole('heading', { name: 'Stats', level: 1 }).waitFor();
await page.waitForTimeout(1500);
const peopleTops = await page.$$eval('.people-row > section', (els) =>
	els.map((e) => Math.round(e.getBoundingClientRect().top))
);
check(
	'actors, directors and studios side by side',
	peopleTops.length >= 2 && new Set(peopleTops).size === 1,
	`${peopleTops.length} lists at ${peopleTops.join(', ')}`
);
await page.screenshot({ path: join(out, '3c-stats.png'), fullPage: true });

// The new mascots can be chosen, and become the icon.
await page.goto(`${BASE}/settings#appearance`);
await page.getByRole('heading', { name: 'Mascot', level: 2 }).waitFor();
for (const name of ['Starry-eyed', 'Bow', 'Delighted', 'Sprout']) {
	await page.getByRole('button', { name, exact: true }).click();
	const icon = await page.locator('link[rel="icon"]').getAttribute('href');
	const id = icon?.match(/mascots\/(\w+)\.svg/)?.[1];
	const res = await fetch(`${BASE}${icon}`);
	check(`${name} becomes the icon`, res.ok && !!id, `${icon} ${res.status}`);
}
await page.screenshot({ path: join(out, '3d-mascots.png') });
await page.getByRole('button', { name: 'Classic', exact: true }).click();

console.log('\n== Deleting an account ==');
const invite = await call('POST', '/api/invites', { role: 'user' });
await fetch(`${BASE}/api/auth/register`, {
	method: 'POST',
	headers: { 'content-type': 'application/json' },
	body: JSON.stringify({ username: 'passing', password: 'passing-password', invite: invite.code })
});
await page.goto(`${BASE}/admin`);
await page.getByRole('button', { name: 'Delete passing' }).click();
await toast('will be deleted').waitFor();
check(
	'the account leaves the list at once',
	(await page.getByText('passing', { exact: true }).count()) === 0
);
await page.waitForTimeout(8500);
const users = (await call('GET', '/api/admin/users')).users.map((u) => u.username);
check('and is gone once the undo has passed', !users.includes('passing'), users.join(', '));

console.log('\n== Phone ==');
const phone = await signedIn(
	{ width: 412, height: 915 },
	{ deviceScaleFactor: 2.6, isMobile: true, hasTouch: true }
);
await phone.goto(`${BASE}/settings#connections`);
await phone.getByRole('heading', { name: 'Connections', level: 2 }).waitFor();
const overflow = await phone.evaluate(
	() => document.documentElement.scrollWidth > document.documentElement.clientWidth
);
check('Settings fits a phone', !overflow);
await phone.screenshot({ path: join(out, '4-phone-settings.png') });
await phone.goto(`${BASE}/people/${USER}`);
await phone.getByRole('heading', { name: 'Leap Frog', level: 1 }).waitFor();
check(
	'the profile fits a phone',
	!(await phone.evaluate(
		() => document.documentElement.scrollWidth > document.documentElement.clientWidth
	))
);
await phone.screenshot({ path: join(out, '5-phone-profile.png'), fullPage: true });

check('no console errors', errors.length === 0, errors.slice(0, 3).join(' | '));

await browser.close();
const failed = results.filter((r) => !r).length;
console.log(`\n${results.length - failed}/${results.length} checks passed; screenshots in ${out}`);
stop();
process.exit(failed ? 1 : 0);
