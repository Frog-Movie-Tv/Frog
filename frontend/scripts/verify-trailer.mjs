/**
 * Verifies the trailer behind a title (Settings → Watching → Play trailers):
 *
 *   1. the picture lifts only once the video's own clock has moved past the
 *      studio logos, never while YouTube's play button shows,
 *   2. a video that stops (as when autoplay is refused, or it buffers) is
 *      covered again at once, and lifts only once it moves again,
 *   3. the loop back to the start covers it, and it lifts again only once
 *      playing.
 *
 * It reads YouTube's own frame every 100 ms (the play button, the video's
 * time) beside whether the picture is up. With --frames=dir it also keeps a
 * screenshot of the top of the page for every sample, to look at by eye.
 *
 *   node scripts/verify-trailer.mjs [--webkit] [--frames=dir]
 *
 * Needs a release build and the TMDB key; reaches YouTube.
 */
import { chromium, webkit } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const PORT = 8083;
const BASE = `http://127.0.0.1:${PORT}`;
const USER = ['trailer', 'trailer-password-1'];
// Inception and Breaking Bad: both have a long trailer on TMDB.
const TITLES = [
	['movie', 27205],
	['tv', 1396]
];
const FROM = 12;
const useWebkit = process.argv.includes('--webkit');
const framesArg = process.argv.find((a) => a.startsWith('--frames='));
const framesDir = framesArg ? resolve(framesArg.slice('--frames='.length)) : null;

const results = [];
const check = (label, ok, detail = '') => {
	results.push(!!ok);
	console.log(`  [${ok ? 'PASS' : 'FAIL'}] ${label}${detail ? ' — ' + detail : ''}`);
};

function tmdbKey() {
	const env = join(repo, '.env');
	if (!existsSync(env)) return '';
	for (const line of readFileSync(env, 'utf8').split(/\r?\n/)) {
		if (line.startsWith('TMDB_API_KEY=')) return line.slice('TMDB_API_KEY='.length).trim();
	}
	return '';
}
if (!tmdbKey()) {
	console.error('Needs TMDB_API_KEY in .env.');
	process.exit(1);
}

const binary = join(
	repo,
	'backend',
	'target',
	'release',
	process.platform === 'win32' ? 'frog.exe' : 'frog'
);
const dataDir = join(repo, 'frontend', '.trailer-data');
rmSync(dataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
mkdirSync(dataDir, { recursive: true });
const server = spawn(binary, [], {
	stdio: 'ignore',
	env: {
		...process.env,
		DATABASE_URL: `sqlite:${join(dataDir, 't.db').split('\\').join('/')}?mode=rwc`,
		FROG_BIND: `127.0.0.1:${PORT}`,
		FROG_STATIC_DIR: join(repo, 'frontend', 'build'),
		FROG_ADMIN_USERNAME: USER[0],
		FROG_ADMIN_PASSWORD: USER[1],
		TMDB_API_KEY: tmdbKey(),
		RUST_LOG: 'frog=warn'
	}
});
process.on('exit', () => {
	server.kill();
	rmSync(dataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
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
const browser = useWebkit
	? await webkit.launch()
	: await chromium.launch({ channel: chrome ? 'chrome' : undefined });
const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
const page = await context.newPage();
await page.goto(`${BASE}/login`);
await page.getByLabel('Username').fill(USER[0]);
await page.getByLabel('Password').fill(USER[1]);
await page.getByRole('button', { name: 'Sign in' }).click();
await page.waitForURL(/\/(welcome)?$/);
await page.evaluate(() =>
	fetch('/api/settings', {
		method: 'PUT',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ onboarded: true, trailerAutoplay: true })
	})
);

/** The YouTube frame behind the page, once it is there. */
function player() {
	return page.frames().find((f) => /youtube(-nocookie)?\.com\/embed/.test(f.url()));
}

/** One look: is the picture up, and what is YouTube showing. */
async function sample() {
	const lifted = await page.evaluate(() => {
		const b = document.querySelector('.hero .backdrop');
		return !!b && b.classList.contains('under');
	});
	const frame = player();
	let video = null;
	if (frame) {
		video = await frame
			.evaluate(() => {
				const v = document.querySelector('video');
				const button = document.querySelector('.ytp-large-play-button');
				const shown = (el) => {
					if (!el) return false;
					const s = getComputedStyle(el);
					const r = el.getBoundingClientRect();
					return (
						s.display !== 'none' && s.visibility !== 'hidden' && +s.opacity > 0.05 && r.width > 0
					);
				};
				return {
					time: v ? v.currentTime : null,
					paused: v ? v.paused : null,
					button: shown(button)
				};
			})
			.catch(() => null);
	}
	return { lifted, video };
}

let shot = 0;
async function watch(ms, label) {
	const seen = [];
	const end = Date.now() + ms;
	while (Date.now() < end) {
		const s = await sample();
		seen.push(s);
		if (framesDir) {
			mkdirSync(framesDir, { recursive: true });
			await page.screenshot({
				path: join(framesDir, `${label}-${String(shot++).padStart(4, '0')}.png`),
				clip: { x: 0, y: 0, width: 1280, height: 520 }
			});
		}
		await page.waitForTimeout(100);
	}
	return seen;
}

/** Up only over a video that is playing past the logos, never over the button. */
function honest(seen) {
	return seen.filter(
		(s) => s.lifted && (!s.video || s.video.button || s.video.paused || (s.video.time ?? 0) < FROM)
	);
}

for (const [kind, tmdb] of TITLES) {
	const { item } = await (await page.request.get(`${BASE}/api/titles/${kind}/${tmdb}`)).json();
	console.log(`== ${item.name} (${useWebkit ? 'WebKit' : 'Chromium'}) ==`);
	await page.goto(`${BASE}/t/${item.id}`);
	await page.locator('.hero').waitFor();

	// 1. The start.
	const start = await watch(9000, `${kind}-start`);
	const firstUp = start.findIndex((s) => s.lifted);
	const buttonSeen = start.some((s) => s.video?.button);
	check(
		'the picture is up only over a playing video past the logos',
		honest(start).length === 0,
		`${honest(start).length} bad of ${start.length} samples`
	);
	check(
		'it lifts',
		firstUp >= 0,
		firstUp >= 0
			? `after ${(firstUp / 10).toFixed(1)} s, video at ${start[firstUp].video?.time?.toFixed(1)} s`
			: 'never lifted: no trailer, or autoplay refused'
	);
	console.log(
		`    (YouTube's button seen at some point: ${buttonSeen ? 'yes, under the picture' : 'no'})`
	);
	if (firstUp < 0 || !player()) continue;

	// 2. Stopped: covered at once, back only once it moves.
	await player().evaluate(() => document.querySelector('video')?.pause());
	const stopped = await watch(2500, `${kind}-paused`);
	check(
		'a stopped video is covered again',
		stopped.slice(10).every((s) => !s.lifted),
		`up in ${stopped.slice(10).filter((s) => s.lifted).length} of ${stopped.length - 10}`
	);
	await player().evaluate(() => document.querySelector('video')?.play());
	const resumed = await watch(4000, `${kind}-resumed`);
	check(
		'and comes back once it plays',
		resumed.some((s) => s.lifted)
	);
	check('never over the button meanwhile', honest([...stopped, ...resumed]).length === 0);

	// 3. The loop: near the end, it goes back to the start covered.
	const near = await player().evaluate(() => {
		const v = document.querySelector('video');
		if (!v || !v.duration) return false;
		v.currentTime = Math.max(0, v.duration - 4.5);
		return true;
	});
	if (!near) {
		check('the loop', false, 'no duration to seek near');
		continue;
	}
	const loop = await watch(9000, `${kind}-loop`);
	const covered = loop.findIndex((s) => !s.lifted);
	const backUp = loop.findIndex((s, i) => i > covered && s.lifted);
	check('the loop covers the picture', covered >= 0);
	check(
		'and lifts again past the logos',
		backUp > covered,
		backUp > 0 ? `video at ${loop[backUp].video?.time?.toFixed(1)} s` : ''
	);
	check('never over the button through the loop', honest(loop).length === 0);
}

await browser.close();
const failed = results.filter((r) => !r).length;
console.log(failed ? `\n${failed} check(s) failed` : '\nAll checks passed');
process.exit(failed ? 1 : 0);
