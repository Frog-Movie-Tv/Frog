/**
 * DEVELOPMENT ONLY. Seeds a local database with two demo accounts so there is
 * something to look at, and so account isolation can be seen by eye.
 *
 *   node scripts/seed-demo.mjs            seeds data/dev.db
 *   node scripts/seed-demo.mjs --force    deletes it first
 *
 * It starts the server itself, seeds over the API, and stops it again. With a
 * TMDB key in .env each account also gets a small, different library; without
 * one you get the two accounts and nothing else, which is enough to sign in.
 *
 * Never point this at a database you care about.
 */
import { spawn } from 'node:child_process';
import { deflateSync } from 'node:zlib';
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..');
const PORT = 8094;
const BASE = `http://127.0.0.1:${PORT}`;
const force = process.argv.includes('--force');

const ACCOUNTS = [
	{ username: 'mihai', password: 'demo-password-mihai', role: 'owner' },
	{ username: 'zjef', password: 'demo-password-zjef', role: 'admin' }
];

// Deliberately different, so "these two cannot see each other" is obvious.
const LIBRARIES = [
	[
		{ kind: 'tv', tmdb_id: 94605, status: 'watching' },
		{ kind: 'movie', tmdb_id: 27205, status: 'completed', rating: 92 },
		{ kind: 'movie', tmdb_id: 157336, status: 'planned' }
	],
	[
		{ kind: 'tv', tmdb_id: 1396, status: 'completed', rating: 88 },
		{ kind: 'movie', tmdb_id: 603, status: 'completed', rating: 75 },
		{ kind: 'tv', tmdb_id: 66732, status: 'paused' }
	]
];

function tmdbKey() {
	const env = join(repo, '.env');
	if (!existsSync(env)) return '';
	return (
		readFileSync(env, 'utf8')
			.split(/\r?\n/)
			.find((l) => l.startsWith('TMDB_API_KEY='))
			?.slice('TMDB_API_KEY='.length)
			.trim() ?? ''
	);
}

function binary() {
	const exe = process.platform === 'win32' ? 'frog.exe' : 'frog';
	for (const profile of ['release', 'debug']) {
		const path = join(repo, 'backend', 'target', profile, exe);
		if (existsSync(path)) return path;
	}
	console.error('No frog binary. Build it first:\n  cd backend && cargo build');
	process.exit(1);
}

const dataDir = join(repo, 'data');
const dbPath = join(dataDir, 'dev.db');

if (force) {
	for (const suffix of ['', '-wal', '-shm']) rmSync(dbPath + suffix, { force: true });
}
if (existsSync(dbPath)) {
	console.error(
		`${dbPath} already exists.\n` +
			'Run with --force to replace it, or point DATABASE_URL somewhere else.'
	);
	process.exit(1);
}
mkdirSync(dataDir, { recursive: true });

const key = tmdbKey();
const server = spawn(binary(), [], {
	stdio: 'ignore',
	env: {
		...process.env,
		DATABASE_URL: `sqlite:${dbPath.split('\\').join('/')}?mode=rwc`,
		FROG_BIND: `127.0.0.1:${PORT}`,
		FROG_STATIC_DIR: join(repo, 'frontend', 'build'),
		TMDB_API_KEY: key,
		RUST_LOG: 'frog=warn'
	}
});
const stop = () => server.kill();
process.on('exit', stop);

async function waitForServer() {
	for (let i = 0; i < 60; i++) {
		try {
			const res = await fetch(`${BASE}/healthz`);
			if (res.ok) return;
		} catch {
			// not up yet
		}
		await new Promise((r) => setTimeout(r, 250));
	}
	throw new Error('the server never answered /healthz');
}

async function api(path, { method = 'GET', body, cookie } = {}) {
	const res = await fetch(BASE + path, {
		method,
		headers: {
			...(body ? { 'content-type': 'application/json' } : {}),
			...(cookie ? { cookie } : {})
		},
		body: body ? JSON.stringify(body) : undefined
	});
	const text = await res.text();
	let json;
	try {
		json = JSON.parse(text);
	} catch {
		json = text;
	}
	if (!res.ok) {
		throw new Error(`${method} ${path} -> ${res.status} ${JSON.stringify(json)}`);
	}
	const cookies = res.headers.getSetCookie?.() ?? [];
	const session = cookies.find((c) => c.startsWith('frog_session='))?.split(';')[0];
	return { json, cookie: session };
}

// --- a demo badge, drawn here --------------------------------------------
// A round, lossless PNG, as the badge editor makes one: a frog-green disc
// with a light ring, its edge smoothed. Plain zlib, no image library.

const CRC = new Uint32Array(256).map((_, n) => {
	let c = n;
	for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
	return c >>> 0;
});

function crc32(bytes) {
	let c = 0xffffffff;
	for (const b of bytes) c = CRC[(c ^ b) & 0xff] ^ (c >>> 8);
	return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
	const out = Buffer.alloc(12 + data.length);
	out.writeUInt32BE(data.length, 0);
	out.write(type, 4, 'ascii');
	data.copy(out, 8);
	out.writeUInt32BE(crc32(out.subarray(4, 8 + data.length)), 8 + data.length);
	return out;
}

function badgePng(size) {
	const rows = Buffer.alloc(size * (size * 4 + 1));
	const r = size / 2;
	for (let y = 0; y < size; y++) {
		rows[y * (size * 4 + 1)] = 0;
		for (let x = 0; x < size; x++) {
			const d = Math.hypot(x + 0.5 - r, y + 0.5 - r);
			const edge = Math.min(1, Math.max(0, r - d));
			const ring = d > r * 0.8 && d < r * 0.88;
			const [cr, cg, cb] = ring ? [226, 247, 228] : d < r * 0.3 ? [255, 255, 255] : [79, 187, 88];
			const at = y * (size * 4 + 1) + 1 + x * 4;
			rows[at] = cr;
			rows[at + 1] = cg;
			rows[at + 2] = cb;
			rows[at + 3] = Math.round(edge * 255);
		}
	}
	const head = Buffer.alloc(13);
	head.writeUInt32BE(size, 0);
	head.writeUInt32BE(size, 4);
	head[8] = 8;
	head[9] = 6;
	return Buffer.concat([
		Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
		chunk('IHDR', head),
		chunk('IDAT', deflateSync(rows)),
		chunk('IEND', Buffer.alloc(0))
	]);
}

async function main() {
	await waitForServer();

	const [owner, second] = ACCOUNTS;

	console.log(`creating ${owner.username} (owner)`);
	const setup = await api('/api/auth/setup', {
		method: 'POST',
		body: { username: owner.username, password: owner.password, remember: true }
	});
	const cookies = [setup.cookie];

	console.log(`inviting ${second.username} (${second.role})`);
	const invite = await api('/api/invites', {
		method: 'POST',
		body: { role: second.role },
		cookie: setup.cookie
	});
	const joined = await api('/api/auth/register', {
		method: 'POST',
		body: {
			username: second.username,
			password: second.password,
			remember: true,
			invite: invite.json.code
		}
	});
	cookies.push(joined.cookie);

	if (!key) {
		console.log('\nNo TMDB key in .env, so the libraries are empty.');
	} else {
		// Titles are opened by their TMDB address, which puts them in Frog's
		// catalog; everything after that goes by Frog's own id.
		const frogIds = new Map();
		const frogId = async (kind, tmdbId, cookie) => {
			const key = `${kind}/${tmdbId}`;
			if (!frogIds.has(key)) {
				const detail = await api(`/api/titles/${key}`, { cookie });
				frogIds.set(key, detail.json.item.id);
			}
			return frogIds.get(key);
		};
		for (const [i, titles] of LIBRARIES.entries()) {
			for (const t of titles) {
				process.stdout.write(`  ${ACCOUNTS[i].username}: ${t.kind} ${t.tmdb_id} … `);
				try {
					const id = await frogId(t.kind, t.tmdb_id, cookies[i]);
					await api(`/api/t/${id}/library`, {
						method: 'PUT',
						body: { status: t.status, rating: t.rating ?? null },
						cookie: cookies[i]
					});
					console.log('added');
				} catch (e) {
					console.log(`skipped (${e.message.slice(0, 60)})`);
				}
			}
		}
		// One account has watched part of a show, so progress and the diary
		// have something in them.
		try {
			const id = await frogId('tv', 94605, cookies[0]);
			await api(`/api/t/${id}/episodes/1/watch`, {
				method: 'POST',
				body: { watched: true, when: 'release' },
				cookie: cookies[0]
			});
			console.log('  mihai: season 1 marked watched');
		} catch (e) {
			console.log(`  could not mark a season (${e.message.slice(0, 60)})`);
		}
	}

	// One account wears its own colour, stands its banner up, and holds a
	// badge, so what one profile shows and the other does not is plain to see.
	console.log(`dressing ${second.username}'s profile`);
	const large = badgePng(1024);
	await api('/api/account/profile', {
		method: 'PUT',
		body: {
			public: true,
			profile_color: 'teal',
			profile_shade: 'medium',
			banner_layout: 'vertical'
		},
		cookie: cookies[1]
	});
	if (key) {
		try {
			const lib = await api('/api/library?limit=10', { cookie: cookies[1] });
			const backdrop = lib.json.items.find((i) => i.backdrop_path)?.backdrop_path;
			if (backdrop) {
				await api('/api/account/profile', {
					method: 'PUT',
					body: { banner_backdrop: backdrop },
					cookie: cookies[1]
				});
			}
		} catch (e) {
			console.log(`  no banner (${e.message.slice(0, 60)})`);
		}
	}
	const badge = await api('/api/admin/badges', {
		method: 'POST',
		body: {
			name: 'Early frog',
			description: 'Here from the start',
			settings: {},
			large: large.toString('base64'),
			small: badgePng(128).toString('base64'),
			original: large.toString('base64')
		},
		cookie: cookies[0]
	});
	const zjef = (await api('/api/admin/users', { cookie: cookies[0] })).json.users.find(
		(u) => u.username === second.username
	);
	await api(`/api/admin/badges/${badge.json.id}/holders/${zjef.id}`, {
		method: 'PUT',
		body: {},
		cookie: cookies[0]
	});
	console.log(`  teal, a vertical banner, and the Early frog badge`);

	console.log('\nSeeded ' + dbPath);
	for (const a of ACCOUNTS) {
		console.log(`  ${a.username} / ${a.password}   (${a.role})`);
	}
	console.log('\nThese are demo passwords. Never use this database anywhere real.');
}

main()
	.then(() => {
		stop();
		process.exit(0);
	})
	.catch((e) => {
		console.error('\nseed failed:', e.message);
		stop();
		process.exit(1);
	});
