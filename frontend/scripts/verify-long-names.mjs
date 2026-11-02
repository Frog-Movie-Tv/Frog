/**
 * Long names on a phone, with real titles from TMDB.
 *
 *   node scripts/verify-long-names.mjs
 *
 * First with ordinary names: every page at 320, 360 and 1280 wide, comparing
 * each piece of text with the page's long-word rule on and off, because that
 * rule must only ever touch words too long to fit. Then with very long names
 * written straight into the database: every page at 320 and 360, which must
 * not scroll sideways or lose the search and account buttons.
 *
 * Needs a build and a valid TMDB key. Runs on its own port and database.
 */
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const PORT = 8087;
const BASE = `http://127.0.0.1:${PORT}`;
const PASSWORD = 'long-names-password';

const LONG = {
	username: 'w'.repeat(50),
	friend: 'q'.repeat(50),
	display: 'D'.repeat(40),
	shelf: 'S'.repeat(60),
	title: 'T'.repeat(90)
};

function tmdbKey() {
	const env = join(repo, '.env');
	if (!existsSync(env)) return '';
	for (const line of readFileSync(env, 'utf8').split(/\r?\n/)) {
		if (line.startsWith('TMDB_API_KEY=')) return line.slice('TMDB_API_KEY='.length).trim();
	}
	return '';
}

const binary = ['release', 'debug']
	.map((p) =>
		join(repo, 'backend', 'target', p, process.platform === 'win32' ? 'frog.exe' : 'frog')
	)
	.find(existsSync);
if (!binary) {
	console.error('Build the server first.');
	process.exit(1);
}

const dataDir = join(repo, 'frontend', '.long-names-data');
rmSync(dataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
mkdirSync(dataDir, { recursive: true });
const dbFile = join(dataDir, 'long.db');

const server = spawn(binary, [], {
	env: {
		...process.env,
		DATABASE_URL: `sqlite:${dbFile.split('\\').join('/')}?mode=rwc`,
		FROG_BIND: `127.0.0.1:${PORT}`,
		FROG_STATIC_DIR: join(repo, 'frontend', 'build'),
		FROG_ADMIN_USERNAME: '',
		FROG_ADMIN_PASSWORD: '',
		TMDB_API_KEY: tmdbKey(),
		RUST_LOG: 'frog=warn'
	},
	stdio: 'ignore'
});
process.on('exit', () => server.kill());

const cookies = new Map();
async function call(who, method, path, body) {
	const res = await fetch(BASE + path, {
		method,
		headers: {
			...(body ? { 'content-type': 'application/json' } : {}),
			...(cookies.get(who) ? { cookie: cookies.get(who) } : {})
		},
		body: body ? JSON.stringify(body) : undefined
	});
	const set = res.headers.get('set-cookie');
	if (set) cookies.set(who, set.split(';')[0]);
	if (!res.ok) throw new Error(`${method} ${path}: ${res.status} ${await res.text()}`);
	return res;
}

for (let i = 0; i < 80; i++) {
	const up = await fetch(`${BASE}/api/health`).then(
		(r) => r.ok,
		() => false
	);
	if (up) break;
	await new Promise((r) => setTimeout(r, 250));
}

await call('me', 'POST', '/api/auth/setup', {
	username: 'ordinary',
	password: PASSWORD,
	remember: true
});
const invite = await (await call('me', 'POST', '/api/invites', { role: 'user' })).json();
await call('pal', 'POST', '/api/auth/register', {
	username: 'friend',
	password: PASSWORD,
	invite: invite.code,
	remember: true
});
const person = { public: true, display_name: 'A Friend', bio: 'Watches everything.' };
await call('me', 'PUT', '/api/account/profile', person);
await call('pal', 'PUT', '/api/account/profile', person);

const made = await (
	await call('me', 'POST', '/api/shelves', { name: 'Sunday films', kind: 'manual' })
).json();
const shelf = made.shelf.id;

const SEED = [
	{ kind: 'tv', id: 95396, status: 'watching', rating: 92 },
	{ kind: 'tv', id: 1399, status: 'completed', rating: 88 },
	{ kind: 'movie', id: 157336, status: 'completed', rating: 95 },
	{ kind: 'movie', id: 550, status: 'completed', rating: 90 },
	{ kind: 'movie', id: 496243, status: 'planned' }
];
const titles = [];
for (const e of SEED) {
	const opened = await (await call('me', 'GET', `/api/titles/${e.kind}/${e.id}`)).json();
	const t = opened.item.id;
	titles.push(t);
	const entry = { status: e.status, rating: e.rating ?? null, favourite: true };
	await call('me', 'PUT', `/api/t/${t}/library`, entry);
	await call('pal', 'PUT', `/api/t/${t}/library`, entry);
	await call('me', 'POST', `/api/shelves/${shelf}/items`, { title_id: t });
}
await call('me', 'POST', `/api/t/${titles[0]}/episodes/1/1/watch`, { watched: true });

const routes = (me, pal) => [
	'/',
	'/welcome',
	'/library',
	'/search',
	'/search?q=interstellar',
	'/search?q=' + pal.slice(0, 20),
	'/calendar',
	'/diary',
	'/stats',
	'/shelves',
	`/shelves/${shelf}`,
	'/people',
	`/people/${me}`,
	`/people/${me}/library`,
	`/people/${pal}`,
	`/people/${pal}/library`,
	'/settings#account',
	'/settings#appearance',
	'/settings#home',
	'/settings#watching',
	'/settings#connections',
	'/settings#data',
	'/import',
	'/about',
	'/whats-new',
	'/admin',
	...titles.map((t) => `/t/${t}`)
];

async function settled(page) {
	await page.waitForSelector('main > *');
	await page.waitForFunction(() => !document.querySelector('[aria-busy="true"]'));
	await page.waitForTimeout(400);
}

/** Text the long-word rule put on more lines than it needs without the rule. */
function wordsItMoved(page) {
	return page.evaluate(() => {
		const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
		const pieces = [];
		while (walker.nextNode()) {
			if (walker.currentNode.textContent?.trim()) pieces.push(walker.currentNode);
		}
		const lines = (node) => {
			const range = document.createRange();
			range.selectNodeContents(node);
			return new Set([...range.getClientRects()].map((r) => Math.round(r.top))).size;
		};
		const on = pieces.map(lines);
		document.body.style.overflowWrap = 'normal';
		const off = pieces.map(lines);
		document.body.style.overflowWrap = '';
		return pieces.flatMap((node, i) =>
			on[i] > off[i]
				? [
						`${node.parentElement?.tagName} "${node.textContent?.trim().slice(0, 40)}" ${off[i]}→${on[i]} lines`
					]
				: []
		);
	});
}

function pastTheEdge(page) {
	return page.evaluate(() => {
		const width = document.documentElement.clientWidth;
		const out = [];
		const widest = Math.max(document.documentElement.scrollWidth, document.body.scrollWidth);
		if (widest > width) out.push(`scrolls sideways: ${widest} in ${width}`);
		for (const name of ['Search and commands', 'Account']) {
			const box = document.querySelector(`[aria-label^="${name}"]`)?.getBoundingClientRect();
			if (box && (box.right > width || box.left < 0)) out.push(`${name} is off the screen`);
		}
		return out;
	});
}

const browser = await chromium.launch();
const failures = [];

async function walk(widths, paths, check) {
	for (const width of widths) {
		const context = await browser.newContext({
			viewport: { width, height: 740 },
			hasTouch: width < 760
		});
		const [name, value] = cookies.get('me').split('=');
		await context.addCookies([{ name, value, url: BASE }]);
		const page = await context.newPage();
		for (const path of paths) {
			await page.goto(BASE + path);
			await settled(page).catch(() => failures.push(`${width}px ${path}: never settled`));
			for (const found of await check(page)) failures.push(`${width}px ${path}: ${found}`);
		}
		await context.close();
	}
}

/** Everything that opens over a page, drawn with what it says at that moment. */
async function overlays(page) {
	const found = [];
	const look = async (what, act) => {
		await page.goto(`${BASE}/library`);
		await settled(page);
		await act();
		await page.waitForTimeout(300);
		found.push(...(await wordsItMoved(page)).map((f) => `${what}: ${f}`));
	};
	await look('palette', () => page.getByRole('button', { name: 'Search and commands' }).click());
	await look('account menu', () => page.getByRole('button', { name: 'Account' }).click());
	if (page.viewportSize()?.width < 760) {
		await look('filters', () =>
			page.getByRole('button', { name: 'Filters', exact: false }).first().click()
		);
		await look('More sheet', () => page.getByRole('button', { name: 'More' }).click());
	}
	return found;
}

console.log('ordinary names: the rule moves nothing');
await walk([320, 360, 1280], routes('ordinary', 'friend'), wordsItMoved);
await walk([320, 360, 1280], ['/library'], overlays);

const db = new DatabaseSync(dbFile);
db.exec('PRAGMA busy_timeout = 5000');
db.prepare('UPDATE app_user SET username = ?, display_name = ? WHERE username = ?').run(
	LONG.username,
	LONG.display,
	'ordinary'
);
db.prepare('UPDATE app_user SET username = ?, display_name = ? WHERE username = ?').run(
	LONG.friend,
	LONG.display,
	'friend'
);
db.prepare('UPDATE shelf SET name = ? WHERE id = ?').run(LONG.shelf, shelf);
for (const t of titles) db.prepare('UPDATE title SET name = ? WHERE id = ?').run(LONG.title, t);
db.close();

console.log('long names: nothing past the edge');
await walk([320, 360], routes(LONG.username, LONG.friend), pastTheEdge);

await browser.close();
if (failures.length) {
	console.log(`\n${failures.length} problems:\n${[...new Set(failures)].join('\n')}`);
	process.exit(1);
}
console.log('all clear');
process.exit(0);
