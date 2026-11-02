/**
 * Picture uploads the way Safari would meet them.
 *
 *   node scripts/verify-pictures.mjs
 *
 * Safari has no WebP writer: `toBlob` hands back a PNG. So the whole path runs in
 * Chromium twice, as it is and with that writer taken away. WebKit itself runs as
 * far as the upload, because Playwright's build of it on Windows hangs and closes
 * once the page draws the new picture. None of this is a real Safari.
 *
 * Needs `npx playwright install webkit` and a build. Runs on its own port and
 * database.
 */
import { chromium, webkit } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const PORT = 8084;
const BASE = `http://127.0.0.1:${PORT}`;
const USER = 'pictures';
const MAX = 3 * 1024 * 1024;

const binary = ['release', 'debug']
	.map((p) =>
		join(repo, 'backend', 'target', p, process.platform === 'win32' ? 'frog.exe' : 'frog')
	)
	.find(existsSync);
if (!binary) {
	console.error('Build the server first.');
	process.exit(1);
}

const dataDir = join(repo, 'frontend', '.pictures-data');
rmSync(dataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
mkdirSync(dataDir, { recursive: true });
const server = spawn(binary, [], {
	env: {
		...process.env,
		DATABASE_URL: `sqlite:${join(dataDir, 'p.db').split('\\').join('/')}?mode=rwc`,
		FROG_BIND: `127.0.0.1:${PORT}`,
		FROG_STATIC_DIR: join(repo, 'frontend', 'build'),
		FROG_ADMIN_USERNAME: '',
		FROG_ADMIN_PASSWORD: '',
		TMDB_API_KEY: '',
		RUST_LOG: 'off'
	},
	stdio: 'ignore'
});
process.on('exit', () => server.kill());

for (let i = 0; i < 80; i++) {
	if (
		await fetch(`${BASE}/healthz`).then(
			(r) => r.ok,
			() => false
		)
	) {
		break;
	}
	await new Promise((r) => setTimeout(r, 250));
}
const made = await fetch(`${BASE}/api/auth/setup`, {
	method: 'POST',
	headers: { 'content-type': 'application/json' },
	body: JSON.stringify({ username: USER, password: 'pictures-password', remember: true })
});
const cookie = made.headers.get('set-cookie').split(';')[0];
await fetch(`${BASE}/api/settings`, {
	method: 'PUT',
	headers: { 'content-type': 'application/json', cookie },
	body: JSON.stringify({ onboarded: true })
});

/**
 * Draw a picture in the page and hand it to the upload field, firing the field's
 * own change. Half left clear; `noisy` makes it compress badly. Playwright's file
 * chooser does not work with its WebKit build on Windows.
 */
function hand(page, field, name, width, height, noisy) {
	return page.evaluate(
		async ({ field, name, width, height, noisy }) => {
			const canvas = document.createElement('canvas');
			canvas.width = width;
			canvas.height = height;
			const context = canvas.getContext('2d');
			if (noisy) {
				const pixels = context.createImageData(width / 2, height);
				for (let i = 0; i < pixels.data.length; i++) {
					pixels.data[i] = i % 4 === 3 ? 255 : Math.random() * 256;
				}
				context.putImageData(pixels, 0, 0);
			} else {
				context.fillStyle = '#2f9e6e';
				context.fillRect(0, 0, width / 2, height);
			}
			const blob = await new Promise((r) => canvas.toBlob(r, 'image/png'));
			const files = new DataTransfer();
			files.items.add(new File([blob], name, { type: 'image/png' }));
			const input = [...document.querySelectorAll('.file')]
				.find((label) => label.textContent.includes(field))
				.querySelector('input');
			input.files = files.files;
			input.dispatchEvent(new Event('change', { bubbles: true }));
		},
		{ field, name, width, height, noisy }
	);
}

/** What the server holds, as this browser reads it. */
function stored(page, kind) {
	return page.evaluate(
		async ({ user, kind }) => {
			const res = await fetch(`/api/profiles/${user}/images/${kind}?v=${Date.now()}`);
			const blob = await res.blob();
			const bitmap = await createImageBitmap(blob);
			const canvas = document.createElement('canvas');
			canvas.width = bitmap.width;
			canvas.height = bitmap.height;
			const context = canvas.getContext('2d');
			context.drawImage(bitmap, 0, 0);
			return {
				type: res.headers.get('content-type'),
				bytes: blob.size,
				width: bitmap.width,
				height: bitmap.height,
				corner: context.getImageData(bitmap.width - 1, 0, 1, 1).data[3]
			};
		},
		{ user: USER, kind }
	);
}

/**
 * Waiting through `evaluate`, because Playwright's locators and file chooser hang
 * on this page with its WebKit build on Windows, while the page itself is fine.
 */
async function poll(page, check, argument) {
	for (let i = 0; i < 120; i++) {
		if (await page.evaluate(check, argument)) return;
		await new Promise((r) => setTimeout(r, 250));
	}
	throw new Error(`gave up waiting: ${check}`);
}

/** Wait until `count` toasts with these words have been seen. */
function toast(page, words, count) {
	return poll(
		page,
		({ words, count }) =>
			[...document.querySelectorAll('.toast')].filter((t) => t.textContent.includes(words))
				.length >= count,
		{ words, count }
	);
}

const failures = [];
const expect = (ok, what) => {
	if (!ok) failures.push(what);
};

/** A browser that cannot write WebP, as the canvas standard says it then behaves. */
const noWebpWriter = () => {
	const write = HTMLCanvasElement.prototype.toBlob;
	HTMLCanvasElement.prototype.toBlob = function (done, type, quality) {
		write.call(this, done, type === 'image/webp' ? 'image/png' : type, quality);
	};
};

async function open(engine, setup) {
	const browser = await engine.launch();
	const context = await browser.newContext();
	const [key, value] = cookie.split('=');
	await context.addCookies([{ name: key, value, url: BASE }]);
	if (setup) await context.addInitScript(setup);
	const page = await context.newPage();
	await page.goto(`${BASE}/settings#account`);
	await poll(page, () => document.querySelectorAll('.file input').length > 0);
	return { browser, page };
}

for (const [name, setup, expected] of [
	['Chromium', null, 'image/webp'],
	['As Safari, no WebP writer', noWebpWriter, 'image/png']
]) {
	const { browser, page } = await open(chromium, setup);
	console.log(name);

	await hand(page, 'Upload banner', 'clear.png', 3000, 2000, false);
	await toast(page, 'Banner updated', 1);
	const clear = await stored(page, 'banner');
	console.log(`  transparent banner: ${JSON.stringify(clear)}`);
	expect(clear.width === 1800, `${name}: the banner is ${clear.width} wide`);
	expect(clear.corner === 0, `${name}: the clear corner went solid`);
	expect(clear.type === expected, `${name}: the banner went up as ${clear.type}`);

	await hand(page, 'Upload banner', 'noise.png', 3600, 2400, true);
	await toast(page, 'Banner updated', 2);
	const noise = await stored(page, 'banner');
	console.log(`  worst-case banner: ${JSON.stringify(noise)}`);
	expect(noise.bytes <= MAX, `${name}: the worst-case banner is ${noise.bytes} bytes`);

	await hand(page, 'Picture', 'face.png', 1600, 1600, false);
	await poll(page, () =>
		[...document.querySelectorAll('button')].some((b) => b.textContent === 'Use this picture')
	);
	await page.evaluate(() =>
		[...document.querySelectorAll('button')]
			.find((b) => b.textContent === 'Use this picture')
			.click()
	);
	await toast(page, 'Profile picture updated', 1);
	const avatar = await stored(page, 'avatar');
	console.log(`  framed avatar: ${JSON.stringify(avatar)}`);
	expect(avatar.width === 512 && avatar.height === 512, `${name}: the avatar is the wrong size`);
	expect(avatar.corner === 0, `${name}: the avatar's clear corner went solid`);

	await browser.close();
}

{
	await fetch(`${BASE}/api/account/images/banner`, { method: 'DELETE', headers: { cookie } });
	const { browser, page } = await open(webkit);
	console.log('WebKit, as far as the upload');
	const answered = page.waitForResponse(
		(r) => r.request().method() === 'PUT' && r.url().includes('/images/banner')
	);
	await hand(page, 'Upload banner', 'clear.png', 3000, 2000, false);
	const status = (await answered).status();
	const held = await fetch(`${BASE}/api/profiles/${USER}/images/banner`, { headers: { cookie } });
	const bytes = (await held.arrayBuffer()).byteLength;
	console.log(`  answered ${status}: ${held.headers.get('content-type')}, ${bytes} bytes`);
	expect(status === 200, `WebKit: the upload answered ${status}`);
	expect(bytes > 0 && bytes <= MAX, `WebKit: ${bytes} bytes were stored`);
	await Promise.race([browser.close(), new Promise((r) => setTimeout(r, 3000))]);
}

server.kill();
await new Promise((r) => setTimeout(r, 500));
rmSync(dataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
if (failures.length) {
	console.log(`\n${failures.length} problems:\n${failures.join('\n')}`);
	process.exit(1);
}
console.log('all clear');
process.exit(0);
