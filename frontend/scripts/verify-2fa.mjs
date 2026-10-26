/**
 * Two-factor sign-in in real Chrome: turn it on from Settings with a code
 * computed here from the shown key, sign in with a code, fail with a wrong
 * one, sign in with a recovery code, then turn it off again.
 *
 *   node scripts/verify-2fa.mjs
 */
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { createHmac } from 'node:crypto';
import { existsSync, mkdirSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const out = join(repo, 'frontend', 'test-results', 'verify-2fa');
mkdirSync(out, { recursive: true });

const PORT = 8092;
const BASE = `http://127.0.0.1:${PORT}`;
const results = [];
const check = (label, ok, detail = '') => {
	results.push(ok);
	console.log(`  [${ok ? 'PASS' : 'FAIL'}] ${label}${detail ? ' — ' + detail : ''}`);
};

function base32(text) {
	const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
	let bits = '';
	for (const c of text.replace(/\s/g, '')) bits += alphabet.indexOf(c).toString(2).padStart(5, '0');
	const bytes = [];
	for (let i = 0; i + 8 <= bits.length; i += 8) bytes.push(parseInt(bits.slice(i, i + 8), 2));
	return Buffer.from(bytes);
}
function totp(secret, step) {
	const counter = Buffer.alloc(8);
	counter.writeBigInt64BE(BigInt(step));
	const d = createHmac('sha1', secret).update(counter).digest();
	const o = d[19] & 15;
	return String((d.readUInt32BE(o) & 0x7fffffff) % 1e6).padStart(6, '0');
}
const stepNow = () => Math.floor(Date.now() / 1000 / 30);

const binary = join(repo, 'backend', 'target', 'release', process.platform === 'win32' ? 'frog.exe' : 'frog');
const dataDir = join(repo, 'frontend', '.2fa-data');
rmSync(dataDir, { recursive: true, force: true });
mkdirSync(dataDir, { recursive: true });
const server = spawn(binary, [], {
	stdio: 'ignore',
	env: {
		...process.env,
		DATABASE_URL: `sqlite:${join(dataDir, 'a.db').split('\\').join('/')}?mode=rwc`,
		FROG_BIND: `127.0.0.1:${PORT}`,
		FROG_STATIC_DIR: join(repo, 'frontend', 'build'),
		FROG_ADMIN_USERNAME: 'owner',
		FROG_ADMIN_PASSWORD: 'owner-password',
		TMDB_API_KEY: '',
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

async function fresh(viewport = { width: 1280, height: 900 }) {
	const context = await browser.newContext({ viewport });
	const page = await context.newPage();
	page.on('pageerror', (e) => errors.push(e.message));
	return page;
}
async function password(page) {
	await page.goto(`${BASE}/login`);
	await page.getByLabel('Username').fill('owner');
	await page.getByLabel('Password').fill('owner-password');
	await page.getByRole('button', { name: 'Sign in' }).click();
}

console.log('== Turn on ==');
let page = await fresh();
await password(page);
await page.waitForURL(`${BASE}/`);
await page.goto(`${BASE}/settings`);
const card = page.locator('.two-factor');
await card.getByRole('button', { name: 'Turn on' }).click();
await card.locator('.qr svg').waitFor();
check('a QR code is shown', (await card.locator('.qr svg').count()) === 1);
const secretText = (await card.locator('.secret').textContent()).trim();
check('the key is shown for typing by hand', /^[A-Z2-7]{4}( [A-Z2-7]{4}){7}$/.test(secretText), secretText.slice(0, 9) + '…');
const secret = base32(secretText);
await page.screenshot({ path: join(out, '1-scan.png'), fullPage: false });

await card.getByLabel('Code from the app').fill('000000');
await card.getByRole('button', { name: 'Turn on' }).click();
await page.getByText("That code didn't work").first().waitFor();
check('a wrong code does not turn it on', true);

const setupStep = stepNow();
await card.getByLabel('Code from the app').fill(totp(secret, setupStep));
await card.getByRole('button', { name: 'Turn on' }).click();
await card.locator('.codes li').first().waitFor();
const recovery = await card.locator('.codes code').allTextContents();
check('ten recovery codes are shown', recovery.length === 10, recovery[0]);
await page.screenshot({ path: join(out, '2-codes.png') });
await card.getByRole('button', { name: 'Done' }).click();
await card.getByText('On · 10 recovery codes left').waitFor();
check('Settings shows it is on', true);

console.log('\n== Sign in ==');
page = await fresh({ width: 412, height: 915 });
await password(page);
await page.getByLabel('Code').waitFor();
check('after the password, the code is asked for', await page.getByText('Enter the code from your authenticator app.').isVisible());
check('the first ask is not shown as an error', (await page.locator('.notice.error').count()) === 0);
await page.screenshot({ path: join(out, '3-code-phone.png') });

await page.getByLabel('Code').fill('123456');
await page.getByRole('button', { name: 'Verify' }).click();
await page.getByText("That code didn't work.").waitFor();
check('a wrong code is refused', true);

// The setup code's step is spent; the next one is within the drift window.
await page.getByLabel('Code').fill(totp(secret, Math.max(stepNow(), setupStep + 1)));
await page.getByRole('button', { name: 'Verify' }).click();
await page.waitForURL(`${BASE}/`, { timeout: 10000 }).catch(() => {});
check('the right code signs in', page.url() === `${BASE}/`, page.url());

page = await fresh();
await password(page);
await page.getByLabel('Code').fill(recovery[3].toUpperCase());
await page.getByRole('button', { name: 'Verify' }).click();
await page.waitForURL(`${BASE}/`, { timeout: 10000 }).catch(() => {});
check('a recovery code signs in', page.url() === `${BASE}/`, page.url());

console.log('\n== Turn off ==');
await page.goto(`${BASE}/settings`);
await page.locator('.two-factor').getByText('On · 9 recovery codes left').waitFor();
check('the used recovery code is counted', true);
await page.locator('.two-factor').getByRole('button', { name: 'Turn off' }).click();
await page.locator('.two-factor').getByLabel(/Enter your password/).fill('owner-password');
await page.locator('.two-factor form').getByRole('button', { name: 'Turn off' }).click();
await page.locator('.two-factor').getByRole('button', { name: 'Turn on' }).waitFor();
check('turned off with the password', true);

page = await fresh();
await password(page);
await page.waitForURL(`${BASE}/`, { timeout: 10000 }).catch(() => {});
check('the password alone signs in again', page.url() === `${BASE}/`);

check('no page errors', errors.length === 0, errors.slice(0, 2).join(' | '));
await browser.close();
const failed = results.filter((r) => !r).length;
console.log(`\nscreenshots in ${out}\n${failed ? failed + ' check(s) failed' : 'all checks passed'}`);
// The 'exit' handler never runs while the server keeps Node alive.
server.kill();
process.exitCode = failed ? 1 : 0;
