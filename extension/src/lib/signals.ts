// What a page says about what is playing, read the same way on every site.
//
// Nothing here knows a site. Each frame reports what it can see (its own
// address, the addresses of its iframes, JSON-LD, meta tags, the media
// session, its title and headings), and `cluesFrom` turns that into clues,
// each remembering where it came from. `merge` picks the best of them for
// Frog's resolve, structured ids before text. A site rule (rules.ts) can say
// which source to trust on a domain.
//
// Plain TypeScript with no browser APIs, so `node --test` runs it as it is.

export type Kind = 'movie' | 'tv';

/** Where a clue came from. A site rule names one of these, never a selector. */
export type Source =
	| 'page-url'
	| 'frame-url'
	| 'iframe-src'
	| 'jsonld'
	| 'meta'
	| 'media-session'
	| 'title'
	| 'heading';

export const SOURCES: Source[] = [
	'page-url',
	'frame-url',
	'iframe-src',
	'jsonld',
	'meta',
	'media-session',
	'title',
	'heading'
];

/** What one frame can see of itself, as the content script reports it. */
export interface FrameReport {
	/** The frame's own address. Read here, never sent to Frog. */
	url: string;
	/** The `src` of each iframe in it, which a parent can read even across origins. */
	iframes: string[];
	jsonld: unknown[];
	meta: Record<string, string>;
	mediaSession: { title?: string; artist?: string; album?: string } | null;
	title: string;
	headings: string[];
}

export interface Clue {
	source: Source;
	tmdb?: number;
	imdb?: string;
	kind?: Kind;
	season?: number;
	episode?: number;
	/** Counted from the start of the show, as anime sites do. */
	absolute?: number;
	name?: string;
	year?: number;
	/**
	 * An id read from an address shape alone (`/tv/123/1/2`), which may be the
	 * site's own number rather than TMDB's. It is only believed once the name
	 * Frog finds agrees with the page, or a rule says so.
	 */
	weak?: boolean;
}

/** What goes to `/api/ext/resolve`. Only these fields ever leave the browser. */
export interface Signals {
	title_id?: number;
	tmdb_id?: number;
	imdb_id?: string;
	kind?: Kind;
	season?: number;
	episode?: number;
	absolute?: number;
	title?: string;
	year?: number;
	page_title?: string;
}

const MAX = 100_000;
/** A season, an episode or an episode count: small. */
const num = (s: string | null | undefined): number | undefined => {
	if (s == null || !/^\d{1,6}$/.test(s.trim())) return undefined;
	const n = Number(s.trim());
	return n <= MAX ? n : undefined;
};
/** A TMDB id: shows are past 250,000 and films past a million. */
const id = (s: string | null | undefined): number | undefined => {
	if (s == null || !/^\d{1,9}$/.test(s.trim())) return undefined;
	const n = Number(s.trim());
	return n > 0 ? n : undefined;
};
const IMDB = /\btt\d{7,9}\b/i;

// --- text -------------------------------------------------------------------

/** Season and episode in text: S02E07, 2x07, "Season 2 Episode 7", "Episode 7", "Ep 7". */
export function numbersIn(text: string): { season?: number; episode?: number } | null {
	const t = text.toLowerCase();
	let m = t.match(/\bs(\d{1,4})\s*[:.\-]?\s*e(?:p)?(\d{1,5})\b/);
	if (m) return { season: Number(m[1]), episode: Number(m[2]) };
	m = t.match(/\b(\d{1,2})x(\d{1,4})\b/);
	if (m) return { season: Number(m[1]), episode: Number(m[2]) };
	m = t.match(
		/\b(?:season|series|staffel|saison|temporada)\s*(\d{1,4})\D{0,12}?\b(?:episode|ep|folge|épisode|episodio)\.?\s*(\d{1,5})\b/
	);
	if (m) return { season: Number(m[1]), episode: Number(m[2]) };
	m = t.match(/\b(?:episode|ep|folge|épisode|episodio)\.?\s*#?\s*(\d{1,5})\b/);
	if (m) {
		const season = t.match(/\b(?:season|series|staffel|saison|temporada)\s*(\d{1,4})\b/);
		return { season: season ? Number(season[1]) : undefined, episode: Number(m[1]) };
	}
	m = t.match(/(?:^|[\s\-_|:])e(\d{1,4})\b/);
	if (m) return { episode: Number(m[1]) };
	return null;
}

const NOISE = new Set([
	'watch',
	'online',
	'free',
	'stream',
	'streaming',
	'full',
	'hd',
	'sub',
	'dub',
	'subbed',
	'dubbed',
	'english',
	'eng',
	'1080p',
	'720p',
	'4k',
	'now',
	'playing'
]);

/** A name with episode numbers, a bracketed year and "watch … online" taken out. */
export function cleanName(raw: string): string {
	let t = raw
		.replace(/[_.]+/g, ' ')
		.replace(/\bs\d{1,4}\s*[:.\-]?\s*e(?:p)?\d{1,5}\b/gi, ' ')
		.replace(/\b\d{1,2}x\d{1,4}\b/gi, ' ')
		.replace(/\b(?:season|series|episode|ep|part|cour)\.?\s*#?\d{1,5}\b/gi, ' ')
		.replace(/[([]\s*(?:18|19|20)\d\d\s*[)\]]/g, ' ');
	t = t
		.split(/\s+/)
		.filter((w) => w && !NOISE.has(w.toLowerCase().replace(/[^\p{L}\p{N}]/gu, '')))
		.join(' ');
	return t.replace(/^[\s\-–—|:·,]+|[\s\-–—|:·,]+$/g, '').trim();
}

/** The part of a page title that names the work: sites add their name after a bar or dash. */
export function nameFromTitle(title: string): string | undefined {
	const parts = title
		.split(/\s[|–—·•]\s|\s-\s|\s::\s/)
		.map(cleanName)
		.filter((p) => p.length >= 2);
	return parts[0];
}

/** Words compared without case, accents or punctuation. */
export function fold(s: string): string {
	return s
		.normalize('NFKD')
		.replace(/\p{M}/gu, '')
		.toLowerCase()
		.replace(/&/g, ' and ')
		.replace(/[^\p{L}\p{N}]+/gu, ' ')
		.trim();
}

/** Whether a page's name and Frog's agree: one holds all the other's words. */
export function namesAgree(a: string, b: string): boolean {
	const x = fold(a).split(' ').filter(Boolean);
	const y = fold(b).split(' ').filter(Boolean);
	if (!x.length || !y.length) return false;
	const [short, long] = x.length <= y.length ? [x, y] : [y, x];
	return short.every((w) => long.includes(w));
}

// --- addresses ----------------------------------------------------------------

/** What an address says: TMDB and IMDb ids, the kind, season and episode. */
export function fromUrl(raw: string, source: Source): Clue | null {
	let url: URL;
	try {
		url = new URL(raw);
	} catch {
		return null;
	}
	const clue: Clue = { source };
	const q = (names: string[]) => {
		for (const [k, v] of url.searchParams) if (names.includes(k.toLowerCase())) return v;
		return null;
	};

	// Named plainly in the query: as sure as an address gets.
	const tmdb = id(q(['tmdb', 'tmdb_id', 'tmdbid', 'tmdb-id']));
	if (tmdb) clue.tmdb = tmdb;
	const imdb = (q(['imdb', 'imdb_id', 'imdbid']) ?? '').match(IMDB)?.[0];
	if (imdb) clue.imdb = imdb.toLowerCase();
	const type = (q(['type', 'kind', 'media_type', 'mediatype']) ?? '').toLowerCase();
	if (type === 'movie' || type === 'film') clue.kind = 'movie';
	if (['tv', 'series', 'show', 'episode'].includes(type)) clue.kind = 'tv';
	const season = num(q(['s', 'season', 'season_number', 'sea']));
	const episode = num(q(['e', 'ep', 'episode', 'episode_number', 'epi']));
	if (season !== undefined) clue.season = season;
	if (episode !== undefined) clue.episode = episode;

	const path = decodeURIComponent(url.pathname).toLowerCase();
	if (!clue.imdb) {
		const m = path.match(IMDB);
		if (m) clue.imdb = m[0];
	}
	const segments = path.split('/').filter(Boolean);

	// `/tmdb/123`, `/tmdb-123`: the address names TMDB itself.
	segments.forEach((s, i) => {
		if (clue.tmdb) return;
		const inline = s.match(/^tmdb[-_:]?(\d{1,9})$/);
		if (inline) clue.tmdb = Number(inline[1]);
		else if (s === 'tmdb') clue.tmdb = id(segments[i + 1]?.split('-')[0]);
	});

	// `/tv/123/2/7`, `/movie/123`, `/embed/tv/123-name/season/2/episode/7`.
	for (let i = 0; i < segments.length; i++) {
		const s = segments[i];
		const kind: Kind | undefined =
			s === 'movie' || s === 'movies' || s === 'film'
				? 'movie'
				: s === 'tv' || s === 'series' || s === 'show' || s === 'shows'
					? 'tv'
					: undefined;
		if (!kind) continue;
		const found = id(segments[i + 1]?.split('-')[0]);
		if (!found) continue;
		clue.kind ??= kind;
		if (!clue.tmdb) {
			clue.tmdb = found;
			clue.weak = true;
		}
		if (kind === 'tv') {
			const rest = segments.slice(i + 2);
			const a = num(rest[0]);
			const b = num(rest[1]);
			if (a !== undefined && b !== undefined && clue.season === undefined) {
				clue.season = a;
				clue.episode = b;
			}
		}
		break;
	}

	// `/season/2/episode/7`, `/s2/e7`, and slugs like `name-season-2-episode-7`.
	if (clue.episode === undefined) {
		const found = numbersIn(segments.join(' ').replace(/[-_/]/g, ' '));
		if (found) {
			clue.season ??= found.season;
			clue.episode = found.episode;
		}
	}
	if (clue.tmdb || clue.imdb) clue.kind ??= clue.episode !== undefined ? 'tv' : undefined;
	return Object.keys(clue).length > 1 ? clue : null;
}

// --- structured data --------------------------------------------------------------

type Json = Record<string, unknown>;

const asList = (v: unknown): unknown[] => (Array.isArray(v) ? v : v == null ? [] : [v]);
const str = (v: unknown): string | undefined =>
	typeof v === 'string' ? v : typeof v === 'number' ? String(v) : undefined;
const types = (o: Json): string[] => asList(o['@type']).map((t) => String(t).toLowerCase());

/** Every object in a JSON-LD block, @graph and nesting included. */
function objects(value: unknown, out: Json[] = [], depth = 0): Json[] {
	if (depth > 6 || value == null || typeof value !== 'object') return out;
	if (Array.isArray(value)) {
		for (const v of value) objects(v, out, depth + 1);
		return out;
	}
	const o = value as Json;
	out.push(o);
	for (const key of ['@graph', 'mainEntity', 'itemListElement', 'video', 'subjectOf']) {
		if (key in o) objects(o[key], out, depth + 1);
	}
	return out;
}

/** Ids in a list of links: `sameAs`, `url`, `@id`. */
function idsInLinks(o: Json, clue: Clue) {
	for (const link of [...asList(o.sameAs), o.url, o['@id']].map(str).filter(Boolean) as string[]) {
		const imdb = link.match(IMDB)?.[0];
		if (imdb) clue.imdb ??= imdb.toLowerCase();
		const tmdb = link.match(/themoviedb\.org\/(movie|tv)\/(\d+)/i);
		if (tmdb?.[1] && tmdb[2]) {
			clue.tmdb ??= Number(tmdb[2]);
			clue.kind ??= tmdb[1].toLowerCase() as Kind;
		}
	}
}

const yearOf = (v: unknown) => {
	const m = str(v)?.match(/\b(18|19|20)\d\d\b/);
	return m ? Number(m[0]) : undefined;
};

export function fromJsonLd(blocks: unknown[]): Clue[] {
	const out: Clue[] = [];
	for (const o of objects(blocks)) {
		const t = types(o);
		if (t.includes('tvepisode') || t.includes('episode')) {
			const series = (o.partOfSeries ?? (o.partOfSeason as Json | undefined)?.partOfSeries) as
				Json | undefined;
			const season = o.partOfSeason as Json | undefined;
			const clue: Clue = { source: 'jsonld', kind: 'tv' };
			clue.episode = num(str(o.episodeNumber));
			clue.season = num(str(season?.seasonNumber));
			clue.name = str(series?.name) ?? undefined;
			clue.year = yearOf(series?.startDate);
			if (series) idsInLinks(series, clue);
			// An episode's own IMDb id names the episode, which TMDB can place.
			idsInLinks(o, clue);
			out.push(clue);
		} else if (t.includes('tvseries') || t.includes('tvseason')) {
			const clue: Clue = {
				source: 'jsonld',
				kind: 'tv',
				name: str(o.name),
				year: yearOf(o.startDate)
			};
			if (t.includes('tvseason')) clue.season = num(str(o.seasonNumber));
			idsInLinks(o, clue);
			out.push(clue);
		} else if (t.includes('movie')) {
			const clue: Clue = {
				source: 'jsonld',
				kind: 'movie',
				name: str(o.name),
				year: yearOf(o.datePublished ?? o.dateCreated ?? o.releasedEvent)
			};
			idsInLinks(o, clue);
			out.push(clue);
		} else if (t.includes('videoobject')) {
			const name = str(o.name);
			if (name) {
				const clue: Clue = { source: 'jsonld', name: cleanName(name), ...numbersIn(name) };
				out.push(clue);
			}
		}
	}
	return out.filter((c) => Object.keys(c).length > 1);
}

/** OpenGraph, Twitter and `video:*` tags. */
export function fromMeta(meta: Record<string, string>): Clue | null {
	const get = (k: string) => meta[k]?.trim() || undefined;
	const clue: Clue = { source: 'meta' };
	const type = get('og:type');
	if (type === 'video.movie') clue.kind = 'movie';
	if (type === 'video.episode' || type === 'video.tv_show') clue.kind = 'tv';
	const title = get('og:title') ?? get('twitter:title');
	if (title) {
		const n = numbersIn(title);
		if (n) Object.assign(clue, n);
		clue.name = nameFromTitle(title);
	}
	const release = get('video:release_date') ?? get('og:video:release_date');
	if (release) clue.year = yearOf(release);
	const series = get('video:series');
	if (series) {
		const fromSeries = fromUrl(series, 'meta');
		if (fromSeries?.imdb) clue.imdb = fromSeries.imdb;
	}
	for (const v of Object.values(meta)) {
		const imdb = v.match(IMDB)?.[0];
		if (imdb) {
			clue.imdb ??= imdb.toLowerCase();
			break;
		}
	}
	return Object.keys(clue).length > 1 ? clue : null;
}

export function fromMediaSession(m: FrameReport['mediaSession']): Clue | null {
	if (!m) return null;
	const text = [m.title, m.artist, m.album].filter(Boolean).join(' · ');
	if (!text) return null;
	const clue: Clue = { source: 'media-session', ...numbersIn(text) };
	// Players put the show in the album or the artist and the episode in the title.
	const name = [m.album, m.artist, m.title]
		.map((s) => (s ? cleanName(s) : ''))
		.find((s) => s.length >= 2 && !/^\d+$/.test(s));
	if (name) clue.name = name;
	return Object.keys(clue).length > 1 ? clue : null;
}

export function fromText(text: string, source: 'title' | 'heading'): Clue | null {
	if (!text.trim()) return null;
	const clue: Clue = { source, ...numbersIn(text) };
	const name = source === 'title' ? nameFromTitle(text) : cleanName(text);
	if (name) clue.name = name;
	const year = text.match(/[([]\s*((?:18|19|20)\d\d)\s*[)\]]/);
	if (year) clue.year = Number(year[1]);
	const imdb = text.match(IMDB)?.[0];
	if (imdb) clue.imdb = imdb.toLowerCase();
	return Object.keys(clue).length > 1 ? clue : null;
}

/** Every clue in one frame's report. */
export function cluesFrom(report: FrameReport, top: boolean): Clue[] {
	const out: Clue[] = [];
	const push = (c: Clue | null) => c && out.push(c);
	push(fromUrl(report.url, top ? 'page-url' : 'frame-url'));
	for (const src of report.iframes) push(fromUrl(src, 'iframe-src'));
	out.push(...fromJsonLd(report.jsonld));
	push(fromMeta(report.meta));
	push(fromMediaSession(report.mediaSession));
	push(fromText(report.title, 'title'));
	for (const h of report.headings.slice(0, 4)) push(fromText(h, 'heading'));
	return out;
}

// --- choosing -------------------------------------------------------------------

/** Which sources are believed first, for ids and for numbers. */
const ID_ORDER: Source[] = [
	'jsonld',
	'iframe-src',
	'frame-url',
	'page-url',
	'meta',
	'media-session',
	'title',
	'heading'
];
const NUMBER_ORDER: Source[] = [
	'jsonld',
	'iframe-src',
	'frame-url',
	'page-url',
	'media-session',
	'meta',
	'title',
	'heading'
];
const NAME_ORDER: Source[] = ['jsonld', 'meta', 'media-session', 'heading', 'title'];

const rank = (order: Source[]) => (c: Clue) => {
	const i = order.indexOf(c.source);
	return i < 0 ? order.length : i;
};

export interface Merged {
	signals: Signals;
	/** Where each part came from, so a confirmed match can teach a rule. */
	from: { id?: Source; numbers?: Source; name?: Source };
	/** The id was read from an address shape alone. */
	weak: boolean;
}

export interface Preference {
	idFrom?: Source;
	episodeFrom?: Source;
	nameFrom?: Source;
	/** A name this site uses, folded, and Frog's title it means. */
	titles?: Record<string, number>;
}

/** The best of every clue, as resolve takes it. A preference (a site rule) goes first. */
export function merge(clues: Clue[], pageTitle: string, prefer: Preference = {}): Merged {
	const by = (order: Source[], first?: Source) => {
		const r = rank(order);
		return [...clues].sort((a, b) => {
			if (first) {
				if (a.source === first && b.source !== first) return -1;
				if (b.source === first && a.source !== first) return 1;
			}
			// A plainly named id beats one read from an address shape.
			if (!!a.weak !== !!b.weak) return a.weak ? 1 : -1;
			return r(a) - r(b);
		});
	};
	const signals: Signals = {};
	const from: Merged['from'] = {};
	let weak = false;

	const withId = by(ID_ORDER, prefer.idFrom).find((c) => c.tmdb || c.imdb);
	if (withId) {
		if (withId.tmdb) signals.tmdb_id = withId.tmdb;
		if (withId.imdb) signals.imdb_id = withId.imdb;
		if (withId.kind) signals.kind = withId.kind;
		from.id = withId.source;
		weak = !!withId.weak && prefer.idFrom !== withId.source;
		// The numbers that came with the id belong to it.
		if (withId.episode !== undefined) {
			signals.season = withId.season;
			signals.episode = withId.episode;
			from.numbers = withId.source;
		}
	}
	const numbered = by(NUMBER_ORDER, prefer.episodeFrom).find((c) => c.episode !== undefined);
	if (numbered && (from.numbers === undefined || prefer.episodeFrom === numbered.source)) {
		signals.season = numbered.season;
		signals.episode = numbered.episode;
		from.numbers = numbered.source;
	}
	// An episode with no season is counted from the start of the show.
	if (signals.episode !== undefined && signals.season === undefined) {
		signals.absolute = signals.episode;
		delete signals.episode;
	}
	if (signals.season === undefined) delete signals.season;

	const named = by(NAME_ORDER, prefer.nameFrom).find((c) => c.name);
	if (named?.name) {
		signals.title = named.name;
		from.name = named.source;
		const known = prefer.titles?.[fold(named.name)];
		if (known) signals.title_id = known;
	}
	const year = clues.find((c) => c.year)?.year;
	if (year) signals.year = year;
	if (!signals.kind) {
		const kind = by(NAME_ORDER).find((c) => c.kind)?.kind;
		if (kind) signals.kind = kind;
	}
	if (
		signals.kind === undefined &&
		(signals.episode !== undefined || signals.absolute !== undefined)
	) {
		signals.kind = 'tv';
	}
	if (pageTitle.trim()) signals.page_title = pageTitle.trim().slice(0, 300);
	return { signals, from, weak };
}

/** A stable key for a set of signals, so the same page is not resolved twice. */
export function signalsKey(s: Signals): string {
	return JSON.stringify(Object.entries(s).sort(([a], [b]) => a.localeCompare(b)));
}

/** Whether a page has anything at all to go on. */
export function hasAnything(s: Signals): boolean {
	return !!(s.title_id || s.tmdb_id || s.imdb_id || s.title);
}
