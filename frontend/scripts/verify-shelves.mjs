/**
 * Verifies shelves in real Chrome, against real TMDB:
 *
 *   1. a hand-picked shelf is created from the Shelves page, and titles are
 *      added from their own pages (including "New shelf" from that menu),
 *   2. dragging reorders it with a mouse, with a real touch drag on a phone,
 *      and with the arrow keys, and every order survives a reload,
 *   3. removing a title can be undone,
 *   4. a smart shelf built only through the rules editor holds exactly what
 *      the rules say ("shows in progress rated 8 or more"),
 *   5. shelves can be renamed, reordered, deleted and brought back.
 *
 *   node scripts/verify-shelves.mjs     (needs the TMDB key and a release build)
 */
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const out = join(repo, 'frontend', 'test-results', 'verify-shelves');
mkdirSync(out, { recursive: true });

const PORT = 8089;
const BASE = `http://127.0.0.1:${PORT}`;
const USER = 'shelves';
const PASS = 'shelves-password';
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
const dataDir = join(repo, 'frontend', '.shelves-data');
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

/** Frog's own address for a title TMDB names; opening it adds it to Frog. */
const titleApi = async (kind, id) =>
	`/api/t/${(await call('GET', `/api/titles/${kind}/${id}`)).item.id}`;

console.log('== seeding ==');
// Breaking Bad: started and rated 9. The Office: rated 8.5 but not started.
// Chernobyl: finished and rated 9.5. A film rated 10.
await call('PUT', `${await titleApi('tv', 1396)}/library`, { status: 'watching', rating: 90 });
await call('POST', `${await titleApi('tv', 1396)}/episodes/1/1/watch`, { watched: true });
await call('PUT', `${await titleApi('tv', 2316)}/library`, { status: 'planned', rating: 85 });
await call('PUT', `${await titleApi('tv', 87108)}/library`, { status: 'watching', rating: 95 });
await call('POST', `${await titleApi('tv', 87108)}/episodes/watch`, {
	watched: true,
	when: 'unknown'
});
for (const [id, rating] of [
	[278, 100],
	[680, 88],
	[155, 90]
]) {
	await call('PUT', `${await titleApi('movie', id)}/library`, { status: 'completed', rating });
}
check('library seeded', true);

const chrome = existsSync('C:/Program Files/Google/Chrome/Application/chrome.exe');
const browser = await chromium.launch({ channel: chrome ? 'chrome' : undefined });

async function signedIn(viewport, extra = {}) {
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
	return { page, context, errors };
}

const shelfNames = async (id) => (await call('GET', `/api/shelves/${id}`)).items.map((i) => i.name);

/** Drag the handle of item `from` onto item `to` in small steps. */
async function mouseDrag(page, from, to) {
	const handles = page.locator('[data-sort-handle]');
	const a = await handles.nth(from).boundingBox();
	const b = await handles.nth(to).boundingBox();
	await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
	await page.mouse.down();
	for (let i = 1; i <= 12; i++) {
		await page.mouse.move(
			a.x + a.width / 2 + ((b.x - a.x) * i) / 12,
			a.y + a.height / 2 + ((b.y - a.y) * i) / 12
		);
	}
	await page.mouse.up();
}

let comfort;
{
	console.log('\n== hand-picked shelf ==');
	const { page, context, errors } = await signedIn({ width: 1280, height: 900 });
	await page.goto(`${BASE}/shelves`);
	await page.getByText('No shelves yet').waitFor();
	check('the tab is named', (await page.title()) === 'Shelves · Frog');
	check('an empty page offers a first shelf', true);
	check(
		'Shelves is in the navigation',
		(await page.locator('header a[href="/shelves"], nav a[href="/shelves"]').count()) > 0
	);

	await page.getByRole('button', { name: 'New shelf' }).first().click();
	await page.getByLabel('Name').fill('Comfort');
	await page.getByRole('button', { name: 'Create' }).click();
	await page.waitForURL(/\/shelves\/\d+$/);
	comfort = Number(page.url().split('/').pop());
	await page.getByRole('heading', { name: 'Comfort' }).waitFor();
	check('creating opens the new shelf', true);
	await page.getByText('Nothing on this shelf yet').waitFor();
	check('an empty shelf says how to fill it', true);

	for (const id of [278, 680, 155]) {
		await page.goto(`${BASE}/title/movie/${id}`);
		await page.getByRole('button', { name: 'Add to shelf' }).click();
		await page.getByRole('menuitemcheckbox', { name: 'Comfort' }).click();
		await page.getByText('Added to Comfort').waitFor();
	}
	// The menu stays open, so several shelves can be ticked in one go.
	check(
		'the menu shows the title is on the shelf',
		(await page.getByRole('menuitemcheckbox', { name: 'Comfort' }).getAttribute('aria-checked')) ===
			'true'
	);
	await page.screenshot({ path: join(out, '1-add-to-shelf-menu.png') });
	await page.keyboard.press('Escape');

	await page.goto(`${BASE}/title/movie/157336`);
	await page.getByRole('button', { name: 'Add to shelf' }).click();
	await page.getByRole('menuitem', { name: 'New shelf' }).click();
	await page.getByLabel('Name').fill('Nolan');
	await page.getByRole('button', { name: 'Create' }).click();
	await page.getByText('Added Interstellar to Nolan').waitFor();
	check('a shelf can be made from a title page, with the title on it', true);

	check(
		'titles are on the shelf in the order added',
		JSON.stringify(await shelfNames(comfort)) ===
			JSON.stringify(['The Shawshank Redemption', 'Pulp Fiction', 'The Dark Knight']),
		(await shelfNames(comfort)).join(', ')
	);

	await page.goto(`${BASE}/shelves/${comfort}`);
	await page.getByRole('button', { name: 'Edit' }).click();
	await mouseDrag(page, 2, 0);
	await page.waitForTimeout(400);
	const afterDrag = await shelfNames(comfort);
	check(
		'dragging with a mouse reorders and saves',
		afterDrag[0] === 'The Dark Knight',
		afterDrag.join(', ')
	);
	await page.screenshot({ path: join(out, '2-edit-desktop.png') });

	await page.locator('[data-sort-handle]').nth(0).focus();
	await page.keyboard.press('ArrowRight');
	await page.waitForTimeout(400);
	const afterKey = await shelfNames(comfort);
	check('the arrow keys reorder too', afterKey[1] === 'The Dark Knight', afterKey.join(', '));

	await page.reload();
	await page.getByRole('heading', { name: 'Comfort' }).waitFor();
	const shown = await page
		.locator('.grid a')
		.evaluateAll((links) => links.map((a) => a.getAttribute('href')));
	const darkKnight = (await titleApi('movie', 155)).replace('/api', '');
	check('the order survives a reload', shown[1] === darkKnight, shown.join(' '));

	await page.getByRole('button', { name: 'Edit' }).click();
	await page.getByRole('button', { name: 'Remove Pulp Fiction' }).click();
	const toast = page.getByText('Removed Pulp Fiction');
	await toast.waitFor();
	check('removing takes it off', !(await shelfNames(comfort)).includes('Pulp Fiction'));
	await page.getByRole('button', { name: 'Undo' }).click();
	await page.waitForTimeout(600);
	const restored = await shelfNames(comfort);
	check(
		'undo puts it back where it was',
		JSON.stringify(restored) === JSON.stringify(afterKey),
		restored.join(', ')
	);

	await page.getByRole('button', { name: 'Rename' }).click();
	await page.getByLabel('Shelf name').fill('Comfort films');
	await page.getByRole('button', { name: 'Save' }).click();
	await page.getByRole('heading', { name: 'Comfort films' }).waitFor();
	check(
		'renaming saves',
		(await call('GET', `/api/shelves/${comfort}`)).shelf.name === 'Comfort films'
	);

	console.log('\n== on Home ==');
	await page.goto(`${BASE}/`);
	await page.getByRole('heading', { name: 'Your shelves' }).waitFor();
	await page.waitForLoadState('networkidle');
	check(
		'the shelf is on Home once, in the row at the top',
		(await page.getByText('Comfort films').count()) === 1
	);
	check(
		'with no row of its own',
		(await page.getByRole('heading', { name: 'Comfort films' }).count()) === 0
	);
	await page.screenshot({ path: join(out, '2b-home-after.png') });

	check('no errors in the console', errors.length === 0, errors.join(' | '));
	await context.close();
}

{
	console.log('\n== smart shelf ==');
	const { page, context, errors } = await signedIn({ width: 1280, height: 900 });
	await page.goto(`${BASE}/shelves`);
	await page.getByRole('button', { name: 'New shelf' }).first().click();
	await page.getByLabel('Name').fill('Started, rated 8+');
	await page.getByRole('radio', { name: /Smart/ }).click();
	await page.getByRole('button', { name: 'Create' }).click();
	await page.waitForURL(/\/shelves\/\d+\?edit=1$/);
	const smart = Number(new URL(page.url()).pathname.split('/').pop());
	const rules = page.getByRole('region', { name: 'Rules' });
	await rules.waitFor();
	check('a new smart shelf opens its rules', true);

	const pick = async (label, option) => {
		await rules.getByRole('button', { name: label, exact: true }).click();
		await page.getByRole('option', { name: option, exact: true }).click();
	};
	await pick('Type', 'Shows');
	await rules.getByRole('button', { name: 'Progress', exact: true }).click();
	await page.getByRole('option', { name: /In progress/ }).click();
	await pick('Rated at least', '8.0');
	await page.screenshot({ path: join(out, '3-rules.png') });
	await rules.getByRole('button', { name: 'Save rules' }).click();
	await rules.waitFor({ state: 'detached' });
	const names = await shelfNames(smart);
	check(
		'the rules pick exactly the shows in progress rated 8+',
		JSON.stringify(names) === JSON.stringify(['Breaking Bad']),
		names.join(', ')
	);
	const stored = (await call('GET', `/api/shelves/${smart}`)).shelf.rules;
	check(
		'the rules are stored as chosen',
		stored.kind === 'tv' && stored.progress === 'started' && stored.min_rating === 80,
		JSON.stringify(stored)
	);
	check(
		'the page shows what matched',
		(await page
			.locator(`.grid a[href="${(await titleApi('tv', 1396)).replace('/api', '')}"]`)
			.count()) === 1
	);
	await page.screenshot({ path: join(out, '4-smart-shelf.png') });

	await page.getByRole('button', { name: 'Rules' }).click();
	await rules.getByRole('button', { name: 'Progress', exact: true }).click();
	await page.getByRole('option', { name: 'Any', exact: true }).click();
	await rules.getByRole('button', { name: 'Save rules' }).click();
	await rules.waitFor({ state: 'detached' });
	const loosened = await shelfNames(smart);
	check('loosening the rules lets more in', loosened.length === 3, loosened.join(', '));

	console.log('\n== the shelves page ==');
	await page.goto(`${BASE}/shelves`);
	const cards = page.locator('.shelves li');
	await cards.nth(2).waitFor();
	check('three shelves are listed', (await cards.count()) === 3);
	check('a smart shelf says so', (await cards.nth(2).innerText()).includes('Smart'));
	await page.getByRole('button', { name: 'Arrange' }).click();
	await mouseDrag(page, 2, 0);
	await page.waitForTimeout(400);
	const order = (await call('GET', '/api/shelves')).shelves.map((s) => s.name);
	check('dragging reorders the shelves', order[0] === 'Started, rated 8+', order.join(', '));
	await page.getByRole('button', { name: 'Done' }).click();
	await page.screenshot({ path: join(out, '5-shelves-desktop.png') });

	const nolan = (await call('GET', '/api/shelves')).shelves.find((s) => s.name === 'Nolan');
	await page.goto(`${BASE}/shelves/${nolan.id}`);
	await page.getByRole('button', { name: 'Delete shelf' }).click();
	await page.waitForURL(`${BASE}/shelves`);
	check(
		'deleting removes it',
		!(await call('GET', '/api/shelves')).shelves.some((s) => s.name === 'Nolan')
	);
	await page.getByRole('button', { name: 'Undo' }).click();
	await page.waitForURL(/\/shelves\/\d+$/);
	await page.getByRole('heading', { name: 'Nolan' }).waitFor();
	const back = Number(page.url().split('/').pop());
	check(
		'undo brings the shelf back with its titles',
		JSON.stringify(await shelfNames(back)) === JSON.stringify(['Interstellar'])
	);

	check('no errors in the console', errors.length === 0, errors.join(' | '));
	await context.close();
}

{
	console.log('\n== phone ==');
	const { page, context, errors } = await signedIn(
		{ width: 375, height: 812 },
		{ isMobile: true, hasTouch: true }
	);
	for (const path of ['/shelves', `/shelves/${comfort}`]) {
		await page.goto(`${BASE}${path}`);
		await page.locator('h1').waitFor();
		const overflow = await page.evaluate(
			() => document.documentElement.scrollWidth - window.innerWidth
		);
		check(`${path} fits 375px`, overflow <= 0, `${overflow}px too wide`);
	}
	await page.getByRole('button', { name: 'Edit' }).click();
	const before = await shelfNames(comfort);

	// A real touch drag: what a thumb does, not a mouse.
	const cdp = await context.newCDPSession(page);
	const handles = page.locator('[data-sort-handle]');
	const a = await handles.nth(before.length - 1).boundingBox();
	const b = await handles.nth(0).boundingBox();
	const point = (x, y) => [{ x, y, id: 1 }];
	await cdp.send('Input.dispatchTouchEvent', {
		type: 'touchStart',
		touchPoints: point(a.x + a.width / 2, a.y + a.height / 2)
	});
	for (let i = 1; i <= 14; i++) {
		const x = a.x + a.width / 2 + ((b.x - a.x) * i) / 14;
		const y = a.y + a.height / 2 + ((b.y - a.y) * i) / 14;
		await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: point(x, y) });
	}
	await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
	await page.waitForTimeout(500);
	const after = await shelfNames(comfort);
	check(
		'a touch drag reorders on a phone',
		after[0] === before.at(-1),
		`${before.join(', ')} → ${after.join(', ')}`
	);
	const scrolled = await page.evaluate(() => window.scrollY);
	check('the drag did not scroll the page', scrolled === 0, String(scrolled));
	await page.screenshot({ path: join(out, '6-edit-375.png') });
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
