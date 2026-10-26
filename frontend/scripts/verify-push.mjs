/**
 * Verifies Web Push end to end in real Chrome, through Google's real push
 * service:
 *
 *   1. Settings turns notifications on and the server stores the device,
 *   2. "Send a test" is accepted by FCM and arrives decrypted in the worker,
 *   3. a show's bell switches its alerts off, and that survives a reload,
 *   4. an episode of a followed show airing today is announced by the worker
 *      as one notification, and the muted show stays silent,
 *   5. turning notifications off removes the device.
 *
 *   node scripts/verify-push.mjs     (needs the TMDB key and a release build)
 */
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, openSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { DatabaseSync } from 'node:sqlite';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const out = join(repo, 'frontend', 'test-results', 'verify-push');
mkdirSync(out, { recursive: true });

const PORT = 8090;
// Chrome treats localhost as a secure context, which push requires.
const BASE = `http://localhost:${PORT}`;
rmSync(join(out, 'server.log'), { force: true });
const USER = 'push';
const PASS = 'push-password';
const BREAKING_BAD = 1396;
const THE_OFFICE = 2316;
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
const dataDir = join(repo, 'frontend', '.push-data');
const dbFile = join(dataDir, 'p.db');
rmSync(dataDir, { recursive: true, force: true });
mkdirSync(dataDir, { recursive: true });

let server;
async function start() {
	// The server log says what a push service answered when it refuses one.
	const log = openSync(join(out, 'server.log'), 'a');
	server = spawn(binary, [], {
		stdio: ['ignore', log, log],
		env: {
			...process.env,
			DATABASE_URL: `sqlite:${dbFile.split('\\').join('/')}?mode=rwc`,
			FROG_BIND: `127.0.0.1:${PORT}`,
			FROG_STATIC_DIR: join(repo, 'frontend', 'build'),
			FROG_ADMIN_USERNAME: USER,
			FROG_ADMIN_PASSWORD: PASS,
			FROG_NOTIFY_HOUR: '0',
			TMDB_API_KEY: key,
			RUST_LOG: 'frog=info'
		}
	});
	for (let i = 0; i < 80; i++) {
		try {
			if ((await fetch(`${BASE}/api/health`)).ok) return;
		} catch {
			/* starting */
		}
		await new Promise((r) => setTimeout(r, 250));
	}
	throw new Error('server did not start');
}
async function stop() {
	const exited = new Promise((r) => server.once('exit', r));
	server.kill();
	await exited;
}
process.on('exit', () => {
	server?.kill();
	rmSync(dataDir, { recursive: true, force: true });
});

await start();

const profile = mkdtempSync(join(tmpdir(), 'frog-push-'));
const chrome = existsSync('C:/Program Files/Google/Chrome/Application/chrome.exe');
const context = await chromium.launchPersistentContext(profile, {
	channel: chrome ? 'chrome' : undefined,
	// Playwright's default --disable-background-networking also switches off
	// Chrome's connection to the push service: messages would be accepted by
	// FCM and never delivered.
	ignoreDefaultArgs: ['--disable-background-networking'],
	headless: true,
	viewport: { width: 1200, height: 900 }
});
await context.grantPermissions(['notifications'], { origin: BASE });
const page = context.pages()[0] ?? (await context.newPage());
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));

await page.goto(`${BASE}/login`);
await page.getByLabel('Username').fill(USER);
await page.getByLabel('Password').fill(PASS);
await page.getByRole('button', { name: 'Sign in' }).click();
await page.waitForURL(`${BASE}/`);

const api = (method, path, body) =>
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

/**
 * Notifications the worker has shown, by tag. Headless Chrome discards
 * displayed notifications (even one shown directly), so getNotifications()
 * proves nothing there; instead record every showNotification() call the push
 * handler makes. The recorder is re-installed on each look, in case Chrome
 * restarted the idle worker in between.
 */
async function shown() {
	const worker = context.serviceWorkers()[0];
	if (!worker) return [];
	return worker
		.evaluate(() => {
			const reg = self.registration;
			if (!self.__frogShown) {
				self.__frogShown = [];
				const original = reg.showNotification.bind(reg);
				reg.showNotification = (title, options = {}) => {
					self.__frogShown.push({
						tag: options.tag,
						title,
						body: options.body,
						url: options.data?.url
					});
					return original(title, options);
				};
			}
			return self.__frogShown;
		})
		.catch(() => []);
}
async function waitForNotification(tag, ms) {
	const until = Date.now() + ms;
	while (Date.now() < until) {
		const hit = (await shown()).find((n) => n.tag === tag);
		if (hit) return hit;
		await new Promise((r) => setTimeout(r, 1000));
	}
	return null;
}

console.log('== seeding ==');
for (const id of [BREAKING_BAD, THE_OFFICE]) {
	const res = await api('PUT', `/api/titles/tv/${id}/library`, { status: 'watching' });
	check(`following show ${id}`, res.status === 200, String(res.status));
}

console.log('\n== turning on ==');
await page.goto(`${BASE}/settings`);
const card = page.locator('.notifications');
await card.getByRole('button', { name: 'Turn on' }).click();
// Registering with FCM from a fresh profile can take most of a minute.
await card
	.getByText('On for this device')
	.waitFor({ timeout: 90000 })
	.catch(() => undefined);
check(
	'Settings says it is on',
	(await card.getByText('On for this device').count()) === 1,
	await card.innerText()
);
const status = await api('GET', '/api/push');
check('the server stored the device', status.body?.devices === 1, JSON.stringify(status.body));
const endpoint = await page.evaluate(
	async () => (await (await navigator.serviceWorker.ready).pushManager.getSubscription())?.endpoint
);
check(
	'the browser subscribed with a real push service',
	/^https:\/\/\S+googleapis\.com\//.test(endpoint ?? ''),
	endpoint?.slice(0, 50)
);
await page.screenshot({ path: join(out, '1-settings-on.png') });

console.log('\n== test notification ==');
await shown();
await card.getByRole('button', { name: 'Send a test' }).click();
await page
	.getByText('Sent to 1 device')
	.waitFor({ timeout: 20000 })
	.catch(() => undefined);
check('FCM accepted it', (await page.getByText('Sent to 1 device').count()) > 0);
// A brand-new registration can take a while to start receiving.
const test = await waitForNotification('frog-test', 120000);
check(
	'it arrived, decrypted, and the worker showed it',
	test?.title === 'Frog' &&
		/Notifications are on/.test(test?.body ?? '') &&
		test?.url === '/calendar',
	JSON.stringify(test)
);

console.log('\n== per-show switch ==');
await page.goto(`${BASE}/title/tv/${THE_OFFICE}`);
const bell = page.getByRole('button', { name: /New episode alerts/ });
await bell.waitFor();
check(
	'a followed show starts with alerts on',
	(await bell.getAttribute('aria-pressed')) === 'true'
);
await bell.click();
await page.getByRole('button', { name: 'New episode alerts off' }).waitFor();
await page.reload();
await page.getByRole('button', { name: /New episode alerts/ }).waitFor();
check(
	'switching it off survives a reload',
	(await page.getByRole('button', { name: /New episode alerts/ }).getAttribute('aria-pressed')) ===
		'false'
);
await page.screenshot({ path: join(out, '2-bell-off.png') });

console.log('\n== episode announcement ==');
await stop();
const today = new Date();
const localDay = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, '0')}-${String(today.getDate()).padStart(2, '0')}`;
{
	const db = new DatabaseSync(dbFile);
	const moved = db
		.prepare(
			`UPDATE episode SET air_date = ?
			  WHERE season_number = 1 AND episode_number = 1
			    AND title_id IN (SELECT id FROM title WHERE kind = 'tv' AND tmdb_id IN (?, ?))`
		)
		.run(localDay, BREAKING_BAD, THE_OFFICE);
	db.close();
	check('both pilots now air today', moved.changes === 2, String(moved.changes));
}
// The worker's first sweep runs five seconds after start.
await start();
await shown();
const announced = await waitForNotification(`frog-tv-${BREAKING_BAD}`, 120000);
check(
	'the followed show is announced',
	announced?.title === 'Breaking Bad' && announced?.url === `/title/tv/${BREAKING_BAD}`,
	JSON.stringify(announced)
);
check(
	'it names the episode',
	/^S01E01 · Pilot is out today$/.test(announced?.body ?? ''),
	announced?.body
);
const muted = (await shown()).find((n) => n.tag === `frog-tv-${THE_OFFICE}`);
check('the muted show stays silent', !muted, JSON.stringify(muted));

// A second sweep must not repeat it: restart and look at what the server sent.
await stop();
{
	const db = new DatabaseSync(dbFile);
	const sent = db.prepare('SELECT COUNT(*) AS n FROM push_sent').get();
	db.close();
	check('the episode is marked as announced', sent.n === 1, String(sent.n));
}
await start();

console.log('\n== turning off ==');
await page.goto(`${BASE}/settings`);
await card.getByRole('button', { name: 'Turn off' }).click();
await card.getByRole('button', { name: 'Turn on' }).waitFor();
const after = await api('GET', '/api/push');
check('the device is removed', after.body?.devices === 0, JSON.stringify(after.body));
check('no errors on the page', errors.length === 0, errors.join(' | '));

await context.close();
await stop();
rmSync(profile, { recursive: true, force: true });
const failed = results.filter((r) => !r).length;
if (failed) {
	console.log('\n--- server log ---');
	const lines = readFileSync(join(out, 'server.log'), 'utf8')
		.replace(/\x1b\[[0-9;]*m/g, '')
		.split('\n')
		.filter((l) => !/database ready/.test(l));
	console.log(lines.join('\n'));
}
console.log(
	`\nscreenshots in ${out}\n${failed ? failed + ' check(s) failed' : 'all checks passed'}`
);
process.exitCode = failed ? 1 : 0;
