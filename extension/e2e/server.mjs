// Two servers for the end-to-end tests, no real streaming site anywhere.
//
// 8072 serves the fixture pages. It answers as both `localhost` and
// `127.0.0.1`, which a browser treats as two origins, so a page can hold a
// genuinely cross-origin player.
//
// 8071 stands in for Frog's four extension endpoints. It knows two titles
// and records every request, which the tests read from `/__calls`.

import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { dirname, extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const fixtures = join(here, 'fixtures');
export const FIXTURES_PORT = 8072;
export const FROG_PORT = 8071;
export const TOKEN = 'frog_test';

// --- fixture pages ---------------------------------------------------------------

/** Addresses that carry the ids themselves, served by one page each. */
const ROUTES = [
	[/^\/tv\/95396\/2\/7\/?$/, 'url-ids.html'],
	[/^\/embed\/?$/, 'embed.html']
];

const TYPES = {
	'.html': 'text/html; charset=utf-8',
	'.webm': 'video/webm',
	'.js': 'text/javascript'
};

async function fixture(req, res) {
	const path = new URL(req.url, 'http://x').pathname;
	let file = ROUTES.find(([re]) => re.test(path))?.[1] ?? normalize(path).replace(/^[\\/]+/, '');
	if (!file || file.includes('..')) file = 'index.html';
	try {
		const body = await readFile(join(fixtures, file));
		const type = TYPES[extname(file)] ?? 'application/octet-stream';
		// Video needs ranges for seeking.
		const range = req.headers.range?.match(/bytes=(\d+)-(\d*)/);
		if (range && type === 'video/webm') {
			const start = Number(range[1]);
			const end = range[2] ? Number(range[2]) : body.length - 1;
			res.writeHead(206, {
				'content-type': type,
				'content-range': `bytes ${start}-${end}/${body.length}`,
				'accept-ranges': 'bytes',
				'content-length': end - start + 1
			});
			res.end(body.subarray(start, end + 1));
			return;
		}
		res.writeHead(200, {
			'content-type': type,
			'accept-ranges': 'bytes',
			'cache-control': 'no-store'
		});
		res.end(body);
	} catch {
		res.writeHead(404).end('not here');
	}
}

// --- a stand-in Frog ---------------------------------------------------------------

const TITLES = {
	1: { title_id: 1, tmdb: { kind: 'tv', id: 95396 }, name: 'Severance', kind: 'tv', year: 2022 },
	2: { title_id: 2, tmdb: { kind: 'movie', id: 949 }, name: 'Heat', kind: 'movie', year: 1995 }
};
const calls = { resolve: [], log: [], undo: [], me: 0 };
let failing = false;
let nextLog = 1;

const card = (t) => ({ ...t, poster_path: null, in_library: false });

function resolve(s) {
	const show = TITLES[1];
	const numbers = () => {
		if (s.season != null && s.episode != null) return [s.season, s.episode];
		if (s.absolute != null) return [1, s.absolute];
		return [null, null];
	};
	let title = null;
	let confidence = 'guess';
	if (s.title_id && TITLES[s.title_id]) [title, confidence] = [TITLES[s.title_id], 'exact'];
	else if (s.tmdb_id === 95396 || s.imdb_id === 'tt11280740') [title, confidence] = [show, 'exact'];
	else if (s.tmdb_id === 949) [title, confidence] = [TITLES[2], 'exact'];
	else if (/severance/i.test(s.title ?? '')) [title, confidence] = [show, 'likely'];
	else if (/heat/i.test(s.title ?? '')) [title, confidence] = [TITLES[2], 'likely'];
	if (!title) return { match: null, alternatives: [] };
	const [season, episode] = title.kind === 'tv' ? numbers() : [null, null];
	if (title.kind === 'tv' && episode == null && confidence === 'exact') confidence = 'likely';
	return {
		match: { ...card(title), confidence, season, episode, episode_name: null },
		alternatives: Object.values(TITLES)
			.filter((t) => t !== title)
			.map(card)
	};
}

function body(req) {
	return new Promise((ok) => {
		let raw = '';
		req.on('data', (c) => (raw += c));
		req.on('end', () => ok(raw ? JSON.parse(raw) : {}));
	});
}

async function frog(req, res) {
	const send = (status, value) => {
		res.writeHead(status, { 'content-type': 'application/json' });
		res.end(JSON.stringify(value));
	};
	const path = new URL(req.url, 'http://x').pathname;
	if (path === '/__calls') return send(200, calls);
	if (path === '/__reset') {
		calls.resolve = [];
		calls.log = [];
		calls.undo = [];
		calls.me = 0;
		failing = false;
		nextLog = 1;
		return send(200, { ok: true });
	}
	if (path === '/__fail') {
		failing = (await body(req)).fail === true;
		return send(200, { failing });
	}
	if (req.headers.authorization !== `Bearer ${TOKEN}`) return send(401, { error: 'unauthorised' });
	if (failing) return send(503, { error: 'Frog is updating.' });
	if (path === '/api/ext/me') {
		calls.me++;
		return send(200, { username: 'tester', watching: [] });
	}
	if (path === '/api/ext/resolve') {
		const signals = await body(req);
		calls.resolve.push(signals);
		return send(200, resolve(signals));
	}
	if (path === '/api/ext/log') {
		const b = await body(req);
		const log_id = nextLog++;
		calls.log.push({ ...b, log_id });
		return send(200, {
			log_id,
			already: false,
			title_id: b.title_id,
			name: 'x',
			status: 'watching'
		});
	}
	if (path === '/api/ext/undo') {
		calls.undo.push(await body(req));
		return send(200, { ok: true });
	}
	return send(404, { error: 'not found' });
}

createServer(fixture).listen(FIXTURES_PORT);
createServer(frog).listen(FROG_PORT, '127.0.0.1');
console.log(`fixtures on ${FIXTURES_PORT}, Frog stand-in on ${FROG_PORT}`);
