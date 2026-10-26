/**
 * Builds a small demo library in a throwaway database and photographs the app.
 *
 * The point is that the README's screenshots are never stale and never show
 * anyone's real watch history: this runs against its own SQLite file on its
 * own port, seeds a handful of well-known titles, takes the pictures and
 * deletes everything.
 *
 *   npm run screenshots
 *
 * Needs TMDB_API_KEY in the repo's .env, because the artwork comes from TMDB.
 */
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const shots = join(repo, 'docs', 'screenshots');

const PORT = 8096;
const BASE = `http://127.0.0.1:${PORT}`;
const USER = 'demo';
const PASS = 'a-demo-password';

/** Recognisable titles that make the screenshots read at a glance. */
const SEED = [
	{ kind: 'tv', id: 95396, status: 'watching', rating: 92, favourite: true, seasons: [1] },
	{ kind: 'tv', id: 1396, status: 'completed', rating: 97, favourite: true },
	{ kind: 'tv', id: 94605, status: 'watching', rating: 90, seasons: [1] },
	{ kind: 'tv', id: 66732, status: 'paused', rating: 78 },
	{ kind: 'movie', id: 157336, status: 'completed', rating: 95, favourite: true },
	{ kind: 'movie', id: 27205, status: 'completed', rating: 88 },
	{ kind: 'movie', id: 680, status: 'completed', rating: 93 },
	{ kind: 'movie', id: 475557, status: 'completed', rating: 74 },
	{ kind: 'movie', id: 1891, status: 'completed', rating: 91 },
	{ kind: 'movie', id: 496243, status: 'planned' },
	{ kind: 'tv', id: 87108, status: 'planned' },
	{ kind: 'movie', id: 872585, status: 'planned' }
];

function tmdbKey() {
	const env = join(repo, '.env');
	if (!existsSync(env)) return '';
	for (const line of readFileSync(env, 'utf8').split(/\r?\n/)) {
		if (line.startsWith('TMDB_API_KEY=')) return line.slice('TMDB_API_KEY='.length).trim();
	}
	return '';
}

const key = tmdbKey();
if (!key) {
	console.error('No TMDB_API_KEY in .env — screenshots would have no artwork. Stopping.');
	process.exit(1);
}

const binary = ['release', 'debug']
	.map((p) =>
		join(repo, 'backend', 'target', p, process.platform === 'win32' ? 'frog.exe' : 'frog')
	)
	.find(existsSync);

if (!binary) {
	console.error('Build the server first: cd backend && cargo build --release');
	process.exit(1);
}

const dataDir = join(repo, 'frontend', '.screenshot-data');
rmSync(dataDir, { recursive: true, force: true });
mkdirSync(dataDir, { recursive: true });
mkdirSync(shots, { recursive: true });

const server = spawn(binary, [], {
	stdio: 'ignore',
	env: {
		...process.env,
		DATABASE_URL: `sqlite:${join(dataDir, 'demo.db').split('\\').join('/')}?mode=rwc`,
		FROG_BIND: `127.0.0.1:${PORT}`,
		FROG_STATIC_DIR: join(repo, 'frontend', 'build'),
		FROG_ADMIN_USERNAME: '',
		FROG_ADMIN_PASSWORD: '',
		TMDB_API_KEY: key,
		FROG_REGION: 'BE',
		RUST_LOG: 'frog=warn'
	}
});

const cleanup = () => {
	server.kill();
	rmSync(dataDir, { recursive: true, force: true });
};
process.on('exit', cleanup);
process.on('SIGINT', () => process.exit(1));

async function waitForServer() {
	for (let i = 0; i < 80; i++) {
		try {
			const res = await fetch(`${BASE}/api/health`);
			if (res.ok) return;
		} catch {
			// not up yet
		}
		await new Promise((r) => setTimeout(r, 250));
	}
	throw new Error('server never started');
}

let cookie = '';

async function call(method, path, body) {
	const res = await fetch(BASE + path, {
		method,
		headers: {
			...(body ? { 'content-type': 'application/json' } : {}),
			...(cookie ? { cookie } : {})
		},
		body: body ? JSON.stringify(body) : undefined
	});
	const set = res.headers.get('set-cookie');
	if (set) cookie = set.split(';')[0];
	return res;
}

async function seed() {
	await call('POST', '/api/auth/setup', { username: USER, password: PASS, remember: true });

	for (const entry of SEED) {
		process.stdout.write(`  seeding ${entry.kind} ${entry.id}\n`);
		await call('PUT', `/api/titles/${entry.kind}/${entry.id}/library`, {
			status: entry.status,
			rating: entry.rating ?? null,
			favourite: entry.favourite ?? false
		});

		// Part-watch a couple of shows so progress bars and "up next" have
		// something to say.
		for (const season of entry.seasons ?? []) {
			await call('POST', `/api/titles/${entry.kind}/${entry.id}/episodes/${season}/watch`, {
				watched: true
			});
			// Then un-watch the tail, leaving the show mid-season.
			for (const ep of [7, 8, 9]) {
				await call('POST', `/api/titles/${entry.kind}/${entry.id}/episodes/${season}/${ep}/watch`, {
					watched: false
				});
			}
			for (const [ep, rating] of [
				[1, 85],
				[2, 90],
				[3, 70],
				[4, 95],
				[5, 88],
				[6, 92]
			]) {
				await call('PUT', `/api/titles/${entry.kind}/${entry.id}/episodes/${season}/${ep}/state`, {
					rating,
					notes: null
				});
			}
		}
	}
}

let appVersion = '';

async function setTheme(theme) {
	await call('PUT', '/api/settings', { theme, mascot: 'classic', lastSeenVersion: appVersion });
}

const DESKTOP = { width: 1440, height: 900 };
const PHONE = { width: 390, height: 844 };

async function shoot(browser, { name, path, viewport, theme, full = false, wait = 2500 }) {
	await setTheme(theme);

	const context = await browser.newContext({
		viewport,
		deviceScaleFactor: 2,
		colorScheme: theme.includes('light') ? 'light' : 'dark'
	});
	await context.addCookies([
		{
			name: cookie.split('=')[0],
			value: cookie.split('=').slice(1).join('='),
			domain: '127.0.0.1',
			path: '/'
		}
	]);

	const page = await context.newPage();
	await page.goto(BASE + path, { waitUntil: 'networkidle' });
	// Posters stream in through our own proxy; give them a beat to land.
	await page.waitForTimeout(wait);
	await page.evaluate(() => window.scrollTo(0, 0));

	await page.screenshot({ path: join(shots, `${name}.png`), fullPage: full });
	console.log(`  ${name}.png`);
	await context.close();
}

try {
	console.log('starting server…');
	await waitForServer();
	appVersion = (await (await fetch(`${BASE}/api/health`)).json()).version ?? '';

	console.log('seeding a demo library…');
	await seed();

	console.log('taking screenshots…');
	const browser = await chromium.launch();

	await shoot(browser, { name: 'home-desktop-pond', path: '/', viewport: DESKTOP, theme: 'pond' });
	await shoot(browser, {
		name: 'library-desktop-pond',
		path: '/library',
		viewport: DESKTOP,
		theme: 'pond'
	});
	await shoot(browser, {
		name: 'title-desktop-pond',
		path: '/title/tv/95396',
		viewport: DESKTOP,
		theme: 'pond'
	});
	await shoot(browser, {
		name: 'settings-desktop-pond',
		path: '/settings',
		viewport: DESKTOP,
		theme: 'pond'
	});
	await shoot(browser, {
		name: 'stats-desktop-pond',
		path: '/stats',
		viewport: DESKTOP,
		theme: 'pond'
	});
	await shoot(browser, {
		name: 'calendar-desktop-pond',
		path: '/calendar',
		viewport: DESKTOP,
		theme: 'pond'
	});
	await shoot(browser, {
		name: 'diary-desktop-pond',
		path: '/diary',
		viewport: DESKTOP,
		theme: 'pond'
	});
	await shoot(browser, {
		name: 'import-desktop-pond',
		path: '/import',
		viewport: DESKTOP,
		theme: 'pond'
	});
	await shoot(browser, {
		name: 'home-desktop-dark',
		path: '/',
		viewport: DESKTOP,
		theme: 'dark'
	});
	await shoot(browser, {
		name: 'library-desktop-light',
		path: '/library',
		viewport: DESKTOP,
		theme: 'pond-light'
	});

	await shoot(browser, { name: 'home-phone-pond', path: '/', viewport: PHONE, theme: 'pond' });
	await shoot(browser, {
		name: 'library-phone-pond',
		path: '/library',
		viewport: PHONE,
		theme: 'pond'
	});
	await shoot(browser, {
		name: 'title-phone-pond',
		path: '/title/tv/95396',
		viewport: PHONE,
		theme: 'pond'
	});

	await browser.close();
	console.log(`\ndone — ${shots}`);
} catch (error) {
	console.error(error);
	process.exitCode = 1;
}
// The 'exit' handler never runs while the server keeps Node alive.
server.kill();
