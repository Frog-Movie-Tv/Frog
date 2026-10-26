/**
 * Verifies accounts end to end in real Chrome: two people side by side.
 *
 *   1. the owner makes an invite on the admin page; a second person joins
 *      through the join page with it, and the link stops working,
 *   2. each account sees only its own library,
 *   3. a profile stays private until its owner turns it on in Settings,
 *   4. Settings lists your devices, changes your password (other devices are
 *      signed out) and switches account through the account menu,
 *   5. the owner switches the second account off and it is signed out at once.
 *
 *   node scripts/verify-accounts.mjs     (needs the TMDB key and a release build)
 */
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const out = join(repo, 'frontend', 'test-results', 'verify-accounts');
mkdirSync(out, { recursive: true });

const PORT = 8086;
const BASE = `http://127.0.0.1:${PORT}`;
const OWNER = ['mihai', 'owner-password-1'];
const GUEST = ['zjef', 'guest-password-1'];
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
const dataDir = join(repo, 'frontend', '.accounts-data');
rmSync(dataDir, { recursive: true, force: true });
mkdirSync(dataDir, { recursive: true });
const server = spawn(binary, [], {
	stdio: 'ignore',
	env: {
		...process.env,
		DATABASE_URL: `sqlite:${join(dataDir, 'a.db').split('\\').join('/')}?mode=rwc`,
		FROG_BIND: `127.0.0.1:${PORT}`,
		FROG_STATIC_DIR: join(repo, 'frontend', 'build'),
		FROG_ADMIN_USERNAME: OWNER[0],
		FROG_ADMIN_PASSWORD: OWNER[1],
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
const errors = [];

async function person(viewport = { width: 900, height: 900 }) {
	const context = await browser.newContext({ viewport });
	const page = await context.newPage();
	page.on('pageerror', (e) => errors.push(e.message));
	page.on('console', (m) => {
		// A deliberately refused request (403, 401) is not a page error.
		if (m.type() === 'error' && !/status of (401|403|404)/.test(m.text())) errors.push(m.text());
	});
	return { context, page };
}

async function signIn(page, [username, password]) {
	await page.goto(`${BASE}/login`);
	await page.getByLabel('Username').fill(username);
	await page.getByLabel('Password').fill(password);
	await page.getByRole('button', { name: 'Sign in' }).click();
	await page.waitForURL(`${BASE}/`);
}

const api = (page, method, path, body) =>
	page.evaluate(
		async ([method, path, body]) => {
			const res = await fetch(path, {
				method,
				headers: body ? { 'content-type': 'application/json' } : {},
				body: body ? JSON.stringify(body) : undefined
			});
			return { status: res.status, body: await res.json().catch(() => null) };
		},
		[method, path, body]
	);

console.log('== joining with an invite ==');
const owner = await person();
await signIn(owner.page, OWNER);
await owner.page.goto(`${BASE}/admin`);
await owner.page.getByRole('heading', { name: 'Administration' }).waitFor();
check(
	'the owner sees themselves under People',
	(await owner.page.locator('.users li').count()) === 1
);
await owner.page.getByRole('button', { name: 'Make a link' }).click();
const link = (await owner.page.locator('.fresh code').textContent())?.trim() ?? '';
check('an invite link is made', /\/join/.test(link), link.replace(/[A-Za-z0-9_-]{16,}/, '…'));
await owner.page.screenshot({ path: join(out, '1-admin-invite.png'), fullPage: true });

const guest = await person();
await guest.page.goto(link);
await guest.page.getByLabel('Name', { exact: true }).fill(GUEST[0]);
await guest.page.getByLabel('Password').fill(GUEST[1]);
await guest.page.getByRole('button', { name: 'Create my account' }).click();
await guest.page.waitForURL(`${BASE}/`, { timeout: 15000 }).catch(() => undefined);
check('the second person joins and lands on Home', new URL(guest.page.url()).pathname === '/');

const stranger = await person();
await stranger.page.goto(link);
await stranger.page.getByLabel('Name', { exact: true }).fill('someone');
await stranger.page.getByLabel('Password').fill('someone-password');
await stranger.page.getByRole('button', { name: 'Create my account' }).click();
await stranger.page.waitForTimeout(1500);
check('the same link does not work twice', new URL(stranger.page.url()).pathname !== '/');
await stranger.context.close();

console.log('\n== the header at every width ==');
for (const width of [800, 900, 1024, 1280, 1440]) {
	await owner.page.setViewportSize({ width, height: 800 });
	await owner.page.goto(`${BASE}/`);
	const avatar = owner.page.getByRole('button', { name: /^Account:/ });
	await avatar.waitFor();
	const box = await avatar.boundingBox();
	check(
		`the account button fits at ${width}px`,
		box && box.x >= 0 && box.x + box.width <= width,
		JSON.stringify(box)
	);
}
await owner.page.setViewportSize({ width: 900, height: 900 });

console.log('\n== side by side ==');
for (const [kind, id, status] of [
	['movie', 157336, 'completed'],
	['tv', 1396, 'watching']
]) {
	await api(owner.page, 'PUT', `/api/titles/${kind}/${id}/library`, { status });
}
await api(guest.page, 'PUT', '/api/titles/movie/603/library', { status: 'planned' });
await owner.page.goto(`${BASE}/library`);
await guest.page.goto(`${BASE}/library`);
await owner.page.locator('a[href="/title/movie/157336"]').first().waitFor();
await guest.page.locator('a[href="/title/movie/603"]').first().waitFor();
const ownerTitles = await owner.page
	.locator('a[href^="/title/"]')
	.evaluateAll((as) => [...new Set(as.map((a) => a.getAttribute('href')))]);
const guestTitles = await guest.page
	.locator('a[href^="/title/"]')
	.evaluateAll((as) => [...new Set(as.map((a) => a.getAttribute('href')))]);
check(
	'the owner sees only their two titles',
	ownerTitles.length === 2 && !ownerTitles.includes('/title/movie/603'),
	ownerTitles.join(' ')
);
check(
	'the second person sees only their one',
	guestTitles.length === 1 && guestTitles[0] === '/title/movie/603',
	guestTitles.join(' ')
);
await owner.page.screenshot({ path: join(out, '2-owner-library.png') });
await guest.page.screenshot({ path: join(out, '2-guest-library.png') });

console.log('\n== profiles ==');
await guest.page.goto(`${BASE}/people/${OWNER[0]}`);
await guest.page.getByText('No profile here').waitFor();
check('a private profile cannot be seen', true);
await owner.page.goto(`${BASE}/settings`);
await owner.page.getByRole('heading', { name: 'Account' }).waitFor();
await owner.page.getByLabel(/Show my profile/).check();
await owner.page.waitForTimeout(500);
await guest.page.goto(`${BASE}/people`);
await guest.page.getByRole('link', { name: OWNER[0] }).click();
await guest.page.getByRole('heading', { name: OWNER[0] }).waitFor();
check(
	'turned on, it appears under People and opens',
	(await guest.page.getByRole('heading', { name: 'Watching' }).count()) === 1
);
await guest.page.screenshot({ path: join(out, '3-profile.png'), fullPage: true });

console.log('\n== your account ==');
const laptop = await person();
await signIn(laptop.page, GUEST);
await guest.page.goto(`${BASE}/settings`);
await guest.page.getByText('Signed-in devices').waitFor();
await guest.page.waitForTimeout(500);
check('Settings lists both devices', (await guest.page.locator('.devices li').count()) === 2);
await guest.page.screenshot({ path: join(out, '4-account-settings.png'), fullPage: true });
await guest.page.getByRole('button', { name: 'Change', exact: true }).click();
await guest.page.getByLabel('Current password').fill(GUEST[1]);
await guest.page.getByLabel('New password').fill('a-brand-new-pass');
await guest.page.getByRole('button', { name: 'Save password' }).click();
await guest.page.getByText(/Password changed/).waitFor();
check(
	'changing the password signs the other device out',
	(await api(laptop.page, 'GET', '/api/account')).status === 401
);
check('this device stays in', (await api(guest.page, 'GET', '/api/account')).status === 200);
await laptop.context.close();

await guest.page.getByRole('button', { name: /^Account:/ }).click();
await guest.page.getByRole('menuitem', { name: 'Switch account' }).click();
await guest.page.waitForURL(/\/login/);
check('Switch account goes to sign-in', true);
await signIn(guest.page, [GUEST[0], 'a-brand-new-pass']);
check('and the new password works', new URL(guest.page.url()).pathname === '/');

console.log('\n== switching an account off ==');
await owner.page.goto(`${BASE}/admin`);
const row = owner.page.locator('.users li', { hasText: GUEST[0] });
await row.getByRole('button', { name: 'Switch off' }).click();
await owner.page.getByText(`${GUEST[0]} is switched off`).waitFor();
check(
	'the second person is signed out at once',
	(await api(guest.page, 'GET', '/api/account')).status === 401
);
await guest.page.goto(`${BASE}/library`);
await guest.page.waitForURL(/\/login/);
check('and sent to sign-in', true);
await owner.page.screenshot({ path: join(out, '5-admin-people.png'), fullPage: true });

check('no errors in the console', errors.length === 0, errors.join(' | '));
await browser.close();
server.kill();
const failed = results.filter((r) => !r).length;
console.log(
	`\nscreenshots in ${out}\n${failed ? failed + ' check(s) failed' : 'all checks passed'}`
);
process.exitCode = failed ? 1 : 0;
