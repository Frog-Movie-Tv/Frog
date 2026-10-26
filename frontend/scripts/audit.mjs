/**
 * Drives every page of a seeded instance and reports anything that went wrong:
 * console errors, failed requests, uncaught exceptions, and whether a service
 * worker can register.
 *
 *   npm run audit
 *
 * Runs against its own database and port, so it never touches real data.
 */
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');

const PORT = 8095;
const BASE = `http://127.0.0.1:${PORT}`;

const SEED = [
	{ kind: 'tv', id: 95396, status: 'watching', rating: 92, favourite: true, seasons: [1] },

	{ kind: 'movie', id: 157336, status: 'completed', rating: 95, favourite: true },
	{ kind: 'movie', id: 496243, status: 'planned' }
];

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

const dataDir = join(repo, 'frontend', '.audit-data');
rmSync(dataDir, { recursive: true, force: true });
mkdirSync(dataDir, { recursive: true });

const serverLog = [];
const server = spawn(binary, [], {
	env: {
		...process.env,
		DATABASE_URL: `sqlite:${join(dataDir, 'audit.db').split('\\').join('/')}?mode=rwc`,
		FROG_BIND: `127.0.0.1:${PORT}`,
		FROG_STATIC_DIR: join(repo, 'frontend', 'build'),
		FROG_ADMIN_USERNAME: '',
		FROG_ADMIN_PASSWORD: '',
		TMDB_API_KEY: tmdbKey(),
		RUST_LOG: 'frog=info,tower_http=warn'
	}
});
server.stdout.on('data', (d) => serverLog.push(String(d)));
server.stderr.on('data', (d) => serverLog.push(String(d)));

process.on('exit', () => {
	server.kill();
	rmSync(dataDir, { recursive: true, force: true });
});

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

async function waitForServer() {
	for (let i = 0; i < 80; i++) {
		try {
			if ((await fetch(`${BASE}/api/health`)).ok) return;
		} catch {
			/* not yet */
		}
		await new Promise((r) => setTimeout(r, 250));
	}
	throw new Error('server never started');
}

const problems = [];

await waitForServer();
await call('POST', '/api/auth/setup', {
	username: 'audit',
	password: 'audit-password-1',
	remember: true
});

console.log('seeding…');
for (const e of SEED) {
	await call('PUT', `/api/titles/${e.kind}/${e.id}/library`, {
		status: e.status,
		rating: e.rating ?? null,
		favourite: e.favourite ?? false
	});
	for (const season of e.seasons ?? []) {
		await call('POST', `/api/titles/${e.kind}/${e.id}/episodes/${season}/watch`, { watched: true });
		for (const ep of [8, 9]) {
			await call('POST', `/api/titles/${e.kind}/${e.id}/episodes/${season}/${ep}/watch`, {
				watched: false
			});
		}
	}
}

const ROUTES = [
	'/',
	'/library',
	'/search',
	'/calendar',
	'/diary',
	'/stats',
	'/stats?year=2026',
	'/shelves',
	'/settings',
	'/import',
	'/about',
	'/whats-new',
	'/title/tv/95396',
	'/title/movie/157336',
	'/person/73457'
];

const browser = await chromium.launch();

for (const viewport of [
	{ name: 'desktop', width: 1440, height: 900 },
	{ name: 'phone', width: 390, height: 844 }
]) {
	const context = await browser.newContext({
		viewport: { width: viewport.width, height: viewport.height },
		colorScheme: 'dark'
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
	page.on('console', (msg) => {
		if (msg.type() === 'error') problems.push(`[${viewport.name}] console: ${msg.text()}`);
	});
	page.on('pageerror', (err) => problems.push(`[${viewport.name}] uncaught: ${err.message}`));
	page.on('response', (res) => {
		if (res.status() >= 400) {
			problems.push(`[${viewport.name}] HTTP ${res.status()} ${new URL(res.url()).pathname}`);
		}
	});

	for (const route of ROUTES) {
		await page.goto(BASE + route, { waitUntil: 'domcontentloaded', timeout: 20000 }).catch((e) => {
			problems.push(`[${viewport.name}] ${route} failed to load: ${e.message}`);
		});
		await page.waitForTimeout(1400);
		// Scroll the whole page, which is where the blanking shows up.
		await page.evaluate(async () => {
			for (let y = 0; y < document.body.scrollHeight; y += 400) {
				window.scrollTo(0, y);
				await new Promise((r) => setTimeout(r, 40));
			}
			window.scrollTo(0, 0);
		});
	}

	// Can a service worker actually register here?
	const sw = await page.evaluate(async () => {
		try {
			const reg = await navigator.serviceWorker.register('/service-worker.js');
			await reg.unregister();
			return 'registered';
		} catch (e) {
			return `FAILED ${e.name}: ${e.message}`;
		}
	});
	console.log(`[${viewport.name}] service worker: ${sw}`);
	if (!sw.startsWith('registered')) problems.push(`[${viewport.name}] service worker ${sw}`);

	await context.close();
}

await browser.close();

const serverErrors = serverLog
	.join('')
	.split(/\r?\n/)
	.filter((l) => l.includes('ERROR'));

console.log('\n=== server errors ===');
console.log(serverErrors.length ? serverErrors.join('\n') : '(none)');

console.log('\n=== browser problems ===');
if (problems.length) {
	const unique = [...new Set(problems)];
	for (const p of unique) console.log('  ' + p);
	process.exitCode = 1;
} else {
	console.log('(none)');
}

// The 'exit' handler never runs while the server keeps Node alive.
server.kill();
