import type { MatchResult, SearchItem, Status } from '#lib/types';
import { titleKey } from '#lib/titles';
import { newRow, parseCsv, type Incoming } from './common';

/**
 * Netflix's viewing activity.
 *
 * Netflix has no API, only a CSV of plays: one line per episode or film, with
 * the show, season and episode run together into one string ("The Flash:
 * Season 1: Pilot"). Show names, season names and episode names can all hold
 * colons of their own, so nothing here splits on "the first colon". Seasons
 * are found by their marker ("Season 2", "Part ll", "Stranger Things 3"),
 * and where there is none, by how titles branch across the whole file: a
 * show is the point where its plays start to differ.
 */

/** One play, cleaned, before grouping. */
export interface NetflixPlay {
	title: string;
	/** yyyy-mm-dd */
	day: string;
	profile: string | null;
}

export interface NetflixExport {
	plays: NetflixPlay[];
	/** Profile names, when the export has more than one person in it. */
	profiles: string[];
	/** Trailers, previews and plays too short to count. */
	ignored: number;
	/** Lines whose date could not be read. */
	undated: number;
	/** Set when the file never says which comes first, the day or the month. */
	dateNote?: string;
}

export interface NetflixResult {
	rows: Incoming[];
	/** Plays with no show name at all, which nothing can be matched to. */
	skipped: number;
	/** Lines for the preview: what was left out and why. */
	notes: string[];
}

type SeasonKind = 'number' | 'part' | 'named' | 'none';

/** What the matching step needs to know about where a row came from. */
export interface NetflixMeta {
	/** The show as Netflix names it. */
	show: string;
	seasonKind: SeasonKind;
	/** Netflix's own name for the season: "Part 2", "STONE OCEAN". */
	seasonName: string | null;
	/** Lowest and highest episode number, when every episode is called "Episode N". */
	numbers?: [number, number];
	/** Those numbers one by one, in the same order as `days`. */
	numbered?: number[];
	/** A title that might be a film or an episode; TMDB decides. */
	loose?: boolean;
	/** Watched with no episode name at all (a film, or a show's own name). */
	single?: boolean;
	/** The last day each of the row's episodes was watched. */
	days?: string[];
	/** Netflix's "Part 2" as a number, the season to fall back on. */
	part?: number;
}

const DAY_MS = 86_400_000;

/** Byte order marks inside titles, doubled spaces, stray tabs: all Netflix's. */
export function cleanTitle(value: string): string {
	return value.replaceAll(String.fromCharCode(0xfeff), '').replace(/\s+/g, ' ').trim();
}

// ---------------------------------------------------------------- dates

interface RawDate {
	a: number;
	b: number;
	year: number;
	short: boolean;
	iso: boolean;
}

function splitDate(value: string): RawDate | null {
	const v = value.trim();
	const iso = v.match(/^(\d{4})-(\d{1,2})-(\d{1,2})/);
	if (iso) return { a: +iso[2], b: +iso[3], year: +iso[1], short: false, iso: true };
	const m = v.match(/^(\d{1,2})[/.-](\d{1,2})[/.-](\d{4}|\d{2})\b/);
	if (!m) return null;
	return { a: +m[1], b: +m[2], year: +m[3], short: m[3].length === 2, iso: false };
}

const pad = (n: number) => String(n).padStart(2, '0');

function validDay(year: number, month: number, day: number): string | null {
	if (month < 1 || month > 12 || day < 1 || day > 31) return null;
	const d = new Date(Date.UTC(year, month - 1, day));
	if (d.getUTCMonth() !== month - 1) return null;
	return `${year}-${pad(month)}-${pad(day)}`;
}

/**
 * Read a column of Netflix dates.
 *
 * Netflix writes them in the account's locale with two-digit years
 * ("10/23/26" in the US, "23/10/2026" elsewhere), and `new Date` reads
 * "10/23/26" as 1926. Which part is the month is decided once for the whole
 * file: any part over 12 settles it.
 */
export function readDays(values: string[], now: Date): { days: (string | null)[]; note?: string } {
	const raw = values.map(splitDate);
	const firstOver = raw.some((r) => r && !r.iso && r.a > 12);
	const secondOver = raw.some((r) => r && !r.iso && r.b > 12);
	const dayFirst = firstOver && !secondOver;
	const today = now.toISOString().slice(0, 10);

	const days = raw.map((r) => {
		if (!r) return null;
		const [month, day] = r.iso || !dayFirst ? [r.a, r.b] : [r.b, r.a];
		let year = r.year;
		if (r.short) {
			year = 2000 + r.year;
			const guess = validDay(year, month, day);
			if (guess && guess > today) year -= 100;
		}
		return validDay(year, month, day);
	});

	let note: string | undefined;
	if (firstOver && secondOver) note = 'Some dates could not be read the same way as the rest';
	else if (!firstOver && !secondOver && raw.some((r) => r && !r.iso)) {
		note = 'Dates read as month, then day';
	}
	return { days, note };
}

// ---------------------------------------------------------------- reading

function header(rows: string[][]): string[] {
	return rows[0].map((h) => h.trim().toLowerCase());
}

/** Seconds in "HH:MM:SS"; null when it is not that shape. */
function seconds(value: string): number | null {
	const m = value.trim().match(/^(\d+):(\d{2}):(\d{2})$/);
	return m ? +m[1] * 3600 + +m[2] * 60 + +m[3] : null;
}

/**
 * Read either Netflix file: the "Title,Date" viewing history from the
 * account page, or ViewingActivity.csv from a personal-data request, which
 * has every profile, the start time and how long each play lasted.
 */
export function readNetflix(text: string, now: Date = new Date()): NetflixExport {
	const rows = parseCsv(text);
	if (rows.length < 2) throw new Error('No plays found in that file.');
	const head = header(rows);
	const col = (...names: string[]) => names.map((n) => head.indexOf(n)).find((i) => i >= 0) ?? -1;

	const title = col('title');
	const date = col('date');
	const start = col('start time', 'start time (utc)');
	if (title < 0 || (date < 0 && start < 0)) {
		throw new Error('That is not a Netflix viewing history.');
	}

	if (date >= 0) {
		const body = rows.slice(1);
		const { days, note } = readDays(
			body.map((r) => r[date] ?? ''),
			now
		);
		const plays: NetflixPlay[] = [];
		let undated = 0;
		body.forEach((r, i) => {
			const day = days[i];
			if (!day) undated++;
			else plays.push({ title: cleanTitle(r[title] ?? ''), day, profile: null });
		});
		return { plays, profiles: [], ignored: 0, undated, dateNote: note };
	}

	// The personal-data export. Its columns have moved over the years, so
	// everything past the title and start time is optional.
	const profile = col('profile name', 'profile');
	const duration = col('duration');
	const extra = col('supplemental video type');
	const plays: NetflixPlay[] = [];
	const profiles = new Set<string>();
	let ignored = 0;
	let undated = 0;
	for (const r of rows.slice(1)) {
		const kind = extra >= 0 ? (r[extra] ?? '').trim() : '';
		const length = duration >= 0 ? seconds(r[duration] ?? '') : null;
		// Trailers, hooks and recaps are not watching; neither is a play that
		// stopped inside two minutes.
		if (kind || (length !== null && length < 120)) {
			ignored++;
			continue;
		}
		const day = (r[start] ?? '').trim().match(/^(\d{4})-(\d{2})-(\d{2})/);
		if (!day) {
			undated++;
			continue;
		}
		const who = profile >= 0 ? (r[profile] ?? '').trim() || null : null;
		if (who) profiles.add(who);
		// This file adds the number to episode names: "Pilot (Episode 1)".
		const name = cleanTitle(r[title] ?? '').replace(/ \((?:Episode|Aflevering) \d+\)$/i, '');
		plays.push({ title: name, day: day[0], profile: who });
	}
	return { plays, profiles: [...profiles].sort(), ignored, undated };
}

// ---------------------------------------------------------------- titles

const WORD_NUMBERS: Record<string, number> = {
	one: 1,
	two: 2,
	three: 3,
	four: 4,
	five: 5,
	six: 6,
	seven: 7,
	eight: 8,
	nine: 9,
	ten: 10
};

const ROMAN: Record<string, number> = {
	I: 1,
	II: 2,
	III: 3,
	IV: 4,
	V: 5,
	VI: 6,
	VII: 7,
	VIII: 8,
	IX: 9,
	X: 10
};

const SEASON =
	/^(?:season|series|seizoen|serie|staffel|saison|temporada|stagione|sezon|volume|vol\.)\s+(\d{1,3})$/i;
const ORDINAL_SEASON =
	/^(\d{1,2})(?:st|nd|rd|th|e|ste|de|\.|ª|º)?\s+(?:season|seizoen|staffel|saison|temporada)$/i;
const BOOK =
	/^(?:book|boek|buch|livre|libro)\s+(\d{1,2}|one|two|three|four|five|six|seven|eight|nine|ten)$/i;
// KENGAN ASHURA's parts are written with a lowercase L: "Part l", "Part ll".
const PART = /^(?:part|deel|teil|parte|partie)\s+(\d{1,2}|[ivx]+|l{1,3}|one|two|three|four|five)$/i;
const LIMITED =
	/^(?:limited series|miniseries|mini-series|miniserie|mini-serie|minisérie|mini-série|serie limitada|série limitée|miniserie|mini series)$/i;
const EPISODE_N = /^(?:episode|aflevering|folge|épisode|episodio)\s+(\d{1,4})$/i;
const CHAPTER = /^chapter\s+\S+/i;

function number(value: string): number | null {
	const v = value.toLowerCase();
	if (/^\d+$/.test(v)) return Number(v);
	if (v in WORD_NUMBERS) return WORD_NUMBERS[v];
	const roman = v.toUpperCase().replace(/L/g, 'I');
	return ROMAN[roman] ?? null;
}

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

interface Marker {
	/** Index of the first segment after the show. */
	at: number;
	/** Index of the first segment of the episode name. */
	next: number;
	season: number;
	kind: 'number' | 'part';
	label: string;
}

/** The first season marker in a title, scanning from the left. */
function findMarker(segs: string[]): Marker | null {
	for (let i = 1; i < segs.length; i++) {
		const s = segs[i];
		let m: RegExpMatchArray | null;
		let marker: Marker | null = null;

		if ((m = s.match(SEASON) ?? s.match(ORDINAL_SEASON) ?? s.match(BOOK))) {
			marker = { at: i, next: i + 1, season: number(m[1])!, kind: 'number', label: s };
		} else if (LIMITED.test(s)) {
			marker = { at: i, next: i + 1, season: 1, kind: 'number', label: s };
		} else if ((m = s.match(PART)) && i + 1 < segs.length && number(m[1]) !== null) {
			// A part needs an episode after it; "The Hangover: Part II" is a film
			// and "Secret Origins: Part 1" an episode.
			marker = { at: i, next: i + 1, season: number(m[1])!, kind: 'part', label: s };
		} else {
			// "Stranger Things: Stranger Things 3: Chapter One".
			const show = segs.slice(0, i).join(': ');
			const rest = segs.slice(i).join(': ');
			const named = rest.match(new RegExp(`^${escape(show)}\\s+(\\d{1,2})(?=: |$)`, 'i'));
			if (named) {
				marker = { at: i, next: i + i, season: Number(named[1]), kind: 'number', label: named[0] };
			}
		}

		if (!marker) continue;
		// "BAKI: Season 1: Part 2: …" is the second half of one season.
		const after = segs[marker.next];
		if (marker.kind === 'number' && after && PART.test(after) && marker.next + 1 < segs.length) {
			marker.next++;
		}
		return marker;
	}
	return null;
}

const LANGUAGES =
	'hindi|telugu|tamil|malayalam|kannada|bengali|marathi|punjabi|gujarati|english|spanish|castilian|french|german|japanese|korean|dutch|portuguese|italian|turkish|arabic|mandarin|cantonese|chinese|thai|indonesian|polish|russian|ukrainian|tagalog|filipino';
const LANGUAGE_TAG = new RegExp(
	`\\s*\\((?:(?:${LANGUAGES})(?:\\s+(?:version|dubbed|dub|audio))?|dubbed|subtitled|original version)\\)$`,
	'i'
);
const YEAR_TAG = /\s*\((\d{4})\)$/;

/** "Agneepath (2012)" searches as Agneepath from 2012; "Doctor (Telugu)" as Doctor. */
export function searchName(value: string): { name: string; year: number | null } {
	let name = value.replace(LANGUAGE_TAG, '');
	let year: number | null = null;
	const m = name.match(YEAR_TAG);
	if (m) {
		year = Number(m[1]);
		name = name.slice(0, m.index);
	}
	return { name: name.trim() || value, year };
}

// ---------------------------------------------------------------- grouping

interface Node {
	name: string;
	children: Map<string, Node>;
	/** Titles that end exactly here. */
	ends: string[];
	/** Every title at or below. */
	total: number;
}

const node = (name: string): Node => ({ name, children: new Map(), ends: [], total: 0 });

function insert(root: Node, segs: string[], title: string) {
	let at = root;
	at.total++;
	for (const s of segs) {
		const key = s.toLowerCase();
		let child = at.children.get(key);
		if (!child) at.children.set(key, (child = node(s)));
		child.total++;
		at = child;
	}
	at.ends.push(title);
}

/** Every title at or below a node, with the path below it. */
function below(n: Node, path: string[] = []): { title: string; rest: string[] }[] {
	const out = n.ends.map((title) => ({ title, rest: path }));
	for (const c of n.children.values()) out.push(...below(c, [...path, c.name]));
	return out;
}

/** "DEATH NOTE: Death Note", "Sun & Moon: Sun & Moon": a season named after its parent. */
const repeats = (parent: string, child: string) =>
	child.toLowerCase().startsWith(parent.toLowerCase());

interface Group {
	show: string[];
	kind: SeasonKind;
	season: number | null;
	seasonName: string | null;
	/** Distinct episode name → full Netflix titles. */
	episodes: Map<string, string[]>;
	/** The same keys → the episode name as Netflix writes it. */
	names: Map<string, string>;
	known: boolean;
}

export function netflixRows(
	data: NetflixExport,
	profile: string | null = null,
	now: Date = new Date()
): NetflixResult {
	const plays = data.plays.filter((p) => profile === null || p.profile === profile);
	const daysOf = new Map<string, Set<string>>();
	let skipped = 0;

	for (const play of plays) {
		const t = play.title;
		// " : Episode 4" and blank titles: Netflix lost the show's name.
		if (!t || t.startsWith(':') || t.split(': ')[0].trim() === '') {
			skipped++;
			continue;
		}
		const set = daysOf.get(t) ?? new Set<string>();
		set.add(play.day);
		daysOf.set(t, set);
	}

	const groups = new Map<string, Group>();
	const group = (
		show: string[],
		kind: SeasonKind,
		season: number | null,
		name: string | null,
		known: boolean
	) => {
		const key = [
			show.join(': ').toLowerCase(),
			kind,
			season ?? '',
			(name ?? '').toLowerCase()
		].join('\u0000');
		let g = groups.get(key);
		if (!g)
			groups.set(
				key,
				(g = {
					show,
					kind,
					season,
					seasonName: name,
					episodes: new Map(),
					names: new Map(),
					known
				})
			);
		return g;
	};
	const add = (g: Group, episode: string, title: string) => {
		const key = episode.toLowerCase();
		const list = g.episodes.get(key) ?? [];
		list.push(title);
		g.episodes.set(key, list);
		if (!g.names.has(key)) g.names.set(key, episode);
	};

	// 1. Titles that name their season. These also tell us which show names
	//    exist, colons and all.
	const singles: string[] = [];
	const unmarked: string[][] = [];
	const known = new Map<string, string[]>();
	for (const title of daysOf.keys()) {
		const segs = title.split(': ');
		if (segs.length === 1) {
			singles.push(title);
			continue;
		}
		const marker = findMarker(segs);
		if (!marker) {
			unmarked.push(segs);
			continue;
		}
		const show = segs.slice(0, marker.at);
		known.set(show.join(': ').toLowerCase(), show);
		const g = group(show, marker.kind, marker.season, marker.label, true);
		add(g, segs.slice(marker.next).join(': ') || title, title);
	}

	// 2. Everything else goes in a tree: under a show Netflix already named,
	//    or under its first segment.
	const roots = new Map<string, { root: Node; show: string[] | null }>();
	for (const segs of unmarked) {
		let showLen = 0;
		for (let i = segs.length - 1; i >= 1; i--) {
			if (known.has(segs.slice(0, i).join(': ').toLowerCase())) {
				showLen = i;
				break;
			}
		}
		const show = showLen ? known.get(segs.slice(0, showLen).join(': ').toLowerCase())! : null;
		const key = show ? `k:${show.join(': ').toLowerCase()}` : `f:${segs[0].toLowerCase()}`;
		let entry = roots.get(key);
		if (!entry) {
			entry = { root: node(show ? show.join(': ') : segs[0]), show };
			roots.set(key, entry);
		}
		insert(entry.root, segs.slice(showLen || 1), segs.join(': '));
	}

	for (const { root, show: knownShow } of roots.values()) {
		// The show is where titles start to branch. "Monster: The Lizzie Borden
		// Story" never does until its episodes, so the whole of it is the name.
		let at = root;
		const show = knownShow ? [...knownShow] : [root.name];
		if (!knownShow) {
			while (at.ends.length === 0 && at.children.size === 1) {
				const only = [...at.children.values()][0];
				if (only.children.size === 0 || only.ends.length > 0) break;
				if (repeats(at.name, only.name)) break;
				show.push(only.name);
				at = only;
			}
		}
		collect(at, show, [], !!knownShow);
	}

	/** Seasons under a show: children that hold several episodes of their own. */
	function collect(n: Node, show: string[], path: string[], isKnown: boolean) {
		const target = path.length
			? group(show, 'named', null, seasonLabel(path), isKnown)
			: group(show, 'none', null, null, isKnown);
		// Beside unnamed episodes, two titles sharing a first part are more
		// likely an episode name with a colon ("Sword and Shield: …") than a
		// season.
		const loose = [...n.children.values()].some((c) => c.ends.length > 0);
		for (const child of n.children.values()) {
			const under = below(child);
			const deeper = under.filter((u) => u.rest.length > 0);
			// One title below is an episode name with a colon in it ("Chapter
			// One: The Vanishing of Will Byers"), unless it is plainly numbered
			// ("Elbaph Arc: Episode 1178").
			const season =
				!CHAPTER.test(child.name) &&
				(deeper.length >= (loose ? 3 : 2) ||
					(under.length === 1 &&
						deeper.length === 1 &&
						deeper[0].rest.length === 1 &&
						EPISODE_N.test(deeper[0].rest[0])));
			if (season) {
				collect(child, show, [...path, child.name], isKnown);
				for (const t of child.ends) add(target, child.name, t);
			} else {
				for (const u of under) add(target, [child.name, ...u.rest].join(': '), u.title);
			}
		}
	}

	// 3. Rows.
	const rows: Incoming[] = [];
	const latest = (titles: Iterable<string>) => {
		let best = '';
		const days = new Set<string>();
		for (const t of titles) {
			for (const d of daysOf.get(t) ?? []) {
				days.add(d);
				if (d > best) best = d;
			}
		}
		return { best, days };
	};
	const iso = (day: string) => `${day}T20:00:00Z`;
	const idle = new Date(now.getTime() - 182 * DAY_MS).toISOString().slice(0, 10);
	const showLatest = new Map<string, string>();

	for (const g of groups.values()) {
		if (!g.episodes.size) continue;
		const titles = [...g.episodes.values()].flat();
		const key = g.show.join(': ').toLowerCase();
		const { best } = latest(titles);
		if (best > (showLatest.get(key) ?? '')) showLatest.set(key, best);
	}

	for (const g of groups.values()) {
		if (!g.episodes.size) continue;
		const showName = g.show.join(': ');
		const titles = [...g.episodes.values()].flat();

		// A handful of titles with no season, under a name Netflix never uses
		// for a season: as likely "The Lord of the Rings: The Two Towers" as an
		// episode. Each goes to TMDB on its own, and rows that turn out to be
		// the same show are put back together after matching.
		const episodeNames = [...g.episodes.keys()];
		const numbered = episodeNames.every((e) => EPISODE_N.test(e));
		if (
			g.kind === 'none' &&
			g.episodes.size <= SMALL &&
			!numbered &&
			!episodeNames.some((e) => CHAPTER.test(e))
		) {
			for (const [key, same] of g.episodes) rows.push(looseRow(same[0], same, g.names.get(key)!));
			continue;
		}

		const { name, year } = searchName(showName);
		const row = newRow(name, 'Netflix', 'netflix');
		const { best } = latest(titles);
		row.kind = 'tv';
		row.year = year;
		// A Netflix "Part" is not reliably a TMDB season (KENGAN ASHURA's two
		// parts are one season): sent without one, so the episodes decide.
		row.season = g.kind === 'part' ? null : g.season;
		row.progress = g.episodes.size;
		row.watched_at = best ? iso(best) : null;
		row.status = (showLatest.get(showName.toLowerCase()) ?? best) < idle ? 'paused' : 'watching';
		row.label = g.seasonName ? `${showName}: ${g.seasonName}` : showName;
		row.alt_titles = prefixes(g.show);
		row.netflix = { show: showName, seasonKind: g.kind, seasonName: g.seasonName };
		if (g.kind === 'part') row.netflix.part = g.season ?? undefined;

		row.netflix.days = [...g.episodes.values()].map((same) => latest(same).best);
		if (numbered) {
			const n = episodeNames.map((e) => Number(e.match(EPISODE_N)![1]));
			row.netflix.numbers = [Math.min(...n), Math.max(...n)];
			row.netflix.numbered = n;
		}
		if (g.kind === 'part') row.info = `Netflix calls this ${g.seasonName}`;
		if (g.kind === 'named' || g.kind === 'none') row.warning = CHOOSE;
		if (g.kind === 'named') {
			row.info = `Netflix calls this “${g.seasonName}”`;
			row.season_name = g.seasonName;
		}
		// TMDB looks the episodes up by name; "Episode 3" says where it is by itself.
		if (!numbered) row.episodes = [...g.names.values()];
		rows.push(row);
	}

	for (const title of singles) rows.push(looseRow(title, [title], null));

	/** A title sent on its own; `episode` is its episode name when it might be one. */
	function looseRow(title: string, same: string[], episode: string | null): Incoming {
		const { name, year } = searchName(title);
		const row = newRow(name, 'Netflix', 'netflix');
		const { best, days } = latest(same);
		row.year = year;
		row.status = 'completed';
		row.watched_at = best ? iso(best) : null;
		row.rewatch = days.size >= 2;
		row.label = title;
		row.progress = episode ? 1 : null;
		row.alt_titles = episode ? prefixes(title.split(': ')) : [];
		// Without a colon it is nearly always a film: "Bleach" is the 2018
		// film, not the series, whose episodes all have names.
		if (!episode) row.prefer = 'movie';
		row.netflix = {
			show: episode ? title.slice(0, -episode.length).replace(/: $/, '') : title,
			seasonKind: 'none',
			seasonName: null,
			loose: true,
			single: !episode
		};
		if (episode) {
			row.episodes = [episode];
			row.netflix.days = [best];
		}
		return row;
	}

	rows.sort((a, b) => (b.watched_at ?? '').localeCompare(a.watched_at ?? ''));

	const notes: string[] = [];
	if (skipped)
		notes.push(
			`${skipped} ${skipped === 1 ? 'play has' : 'plays have'} no show name and ${skipped === 1 ? 'was' : 'were'} left out`
		);
	if (data.ignored) notes.push(`${data.ignored} trailers and short plays left out`);
	if (data.undated)
		notes.push(
			`${data.undated} ${data.undated === 1 ? 'line' : 'lines'} with no readable date left out`
		);
	if (data.dateNote) notes.push(data.dateNote);
	return { rows, skipped, notes };
}

export const CHOOSE = 'Choose which season this is';

/** Up to this many unnamed titles under one name are sent to TMDB one by one. */
const SMALL = 6;

/** A season's path in the tree as a name, without repeats: "Sun & Moon: Ultra Adventures". */
function seasonLabel(path: string[]): string {
	return path.filter((s, i) => i === 0 || s.toLowerCase() !== path[i - 1].toLowerCase()).join(': ');
}

/** Shorter names to fall back on, longest first: "Monster: The Lizzie Borden Story" → ["Monster"]. */
function prefixes(segs: string[]): string[] {
	const full = searchName(segs.join(': ')).name;
	const out: string[] = [];
	for (let i = segs.length - 1; i >= 1; i--) out.push(searchName(segs.slice(0, i).join(': ')).name);
	return [...new Set(out)].filter((p) => p !== full);
}

// ---------------------------------------------------------------- after matching

interface Settling {
	incoming: Incoming;
	match: MatchResult | null;
	chosen: SearchItem | null;
}

const range = (from: number, to: number) =>
	new RegExp(`[${String.fromCharCode(from)}-${String.fromCharCode(to)}]`, 'g');
/** One Piece's "Egghead Arc ①". */
const CIRCLED = range(0x2460, 0x2473);
/** What NFKD splits off "é" and "ō". */
const ACCENTS = range(0x0300, 0x036f);

/** Season names compared loosely: "Sun & Moon - Ultra Adventures" is "Sun & Moon: Ultra Adventures". */
function norm(value: string): string {
	return (
		value
			// Before NFKD, which would turn the circled number into a plain one.
			.replace(CIRCLED, '')
			.normalize('NFKD')
			.replace(ACCENTS, '')
			.toLowerCase()
			// "Part" and "Season" stay: Netflix's "Part 2" is not TMDB's "Season 2".
			.replace(/\b(?:the|arc|saga)\b/g, ' ')
			.replace(/[^a-z0-9]+/g, ' ')
			.trim()
	);
}

/**
 * A TMDB season whose name is Netflix's name for it: the same words
 * ("Sun & Moon: Ultra Adventures" and "Sun & Moon - Ultra Adventures"), or
 * one name whole inside the other ("Egghead Arc ①" and "Egghead Island").
 * Only ever one season; two candidates is no answer.
 *
 * With `show`, the show's own words are left out on both sides, so Netflix's
 * "Beyblade Burst Turbo" finds TMDB's "Turbo" in "Beyblade Burst", and the
 * whole name is compared, not its tail.
 */
function seasonByName(name: string, seasons: SeasonInfo[], show?: string): SeasonInfo | null {
	const base = show ? norm(show) : '';
	const strip = (v: string) =>
		!base ? v : v === base ? '' : v.startsWith(`${base} `) ? v.slice(base.length + 1) : v;
	const wanted = new Set<string>();
	const parts = name.split(': ');
	for (let i = 0; i < (show ? 1 : parts.length); i++) {
		wanted.add(strip(norm(parts.slice(i).join(' '))));
	}
	// "3%" is not "Season 3": a name needs a word to go on.
	for (const w of wanted) if (!/[a-z]{2,}/.test(w)) wanted.delete(w);
	const named = seasons
		.filter((s) => s.number >= 1)
		.map((s) => ({ season: s, words: strip(norm(s.name)) }))
		.filter((s) => s.words);
	const exact = named.filter((s) => wanted.has(s.words));
	if (exact.length) return exact.length === 1 ? exact[0].season : null;
	const within = (a: string, b: string) => ` ${b} `.includes(` ${a} `);
	const loose = named.filter((s) =>
		[...wanted].some((w) => within(w, s.words) || within(s.words, w))
	);
	return loose.length === 1 ? loose[0].season : null;
}

type SeasonInfo = { number: number; name: string; episodes: number };

const watchedOn = (day: string | undefined) => (day ? `${day}T20:00:00Z` : null);

/** What to say when more episodes were watched than TMDB's season has. */
function tooMany(
	watched: number,
	season: number,
	size: number
): { warning?: string; info?: string } {
	if (watched <= size) return {};
	const line = `${watched} episodes watched, and TMDB's season ${season} has ${size}`;
	// One or two over is Netflix counting a recap or a double episode: the
	// whole season is ticked either way. More is a different split.
	return watched - size <= 2
		? { info: `${line}; all ticked` }
		: { warning: `${line}; check the season` };
}

/**
 * Finish Netflix rows once TMDB has answered.
 *
 * Titles sent on their own come back together when they are the same show;
 * named seasons are placed where TMDB uses the same name; and a show is
 * only "completed" when every one of its seasons was seen to the end.
 * Rows are changed in place; the list that comes back has the merged ones
 * removed.
 */
export function settleNetflix<T extends Settling>(rows: T[], now: Date = new Date()): T[] {
	const gone = new Set<T>();
	/** Titles sent on their own that now stand for their show. */
	const promoted = new Set<T>();
	/** Rows split off another because its episodes span several seasons. */
	const extra = new Map<T, T[]>();

	for (const row of rows) {
		const inc = row.incoming;
		if (!inc.netflix || !row.chosen) continue;
		// "Dr.STONE" is "Dr. STONE", "Naruto Shippuden" is "Naruto Shippūden".
		if (
			row.match?.note === 'Closest match by name' &&
			[inc.title, inc.netflix.show, row.match.searched].some(
				(n) => n && sameName(n, row.chosen!.name)
			)
		) {
			row.match.note = null;
		}
		// The episodes TMDB knows by name, each on the day it was watched.
		const days = inc.netflix.days ?? [];
		let placed = row.match?.placed ?? [];
		if (row.chosen.kind !== 'tv' || !placed.length) continue;
		// Episodes found by name inside this show prove it is the show meant.
		const proven = placed.length >= 2 || placed.length === inc.episodes?.length;
		if (proven && row.match?.note === 'Closest match by name') row.match.note = null;
		if (inc.netflix.seasonKind === 'number') {
			// Netflix's season number stands, and names found inside it date
			// its episodes; unless TMDB has no such season, when they say where
			// the episodes are instead.
			if (row.match?.note?.startsWith('TMDB lists no season')) {
				if (!proven) continue;
				row.match.note = null;
			} else {
				placed = placed.filter((p) => p.season === inc.season);
				if (!placed.length) continue;
			}
		}
		inc.watches = placed.map((p) => ({
			season: p.season,
			episode: p.episode,
			watched_at: watchedOn(days[p.index])
		}));
	}

	// Films that turned out to be the same film.
	const films = new Map<string, T>();
	for (const row of rows) {
		if (!row.incoming.netflix || row.chosen?.kind !== 'movie') continue;
		const watched = Number(row.incoming.watched_at?.slice(0, 4));
		const released = row.chosen.year;
		if (released && watched && released > watched) {
			row.incoming.warning = `Released in ${released}, after you watched it; check the match`;
		} else if (row.match?.note === 'There is also a show by this name' && released) {
			// A title with no episode name, out before it was watched: the film.
			row.match.note = null;
		}
		// "Sing: Thriller" found nothing, "Sing" found the film: not the same thing.
		if (row.match?.searched && !row.incoming.warning) {
			row.incoming.warning = `Found by searching “${row.match.searched}”; check the match`;
		}
		const first = films.get(titleKey(row.chosen));
		if (!first) {
			films.set(titleKey(row.chosen), row);
			continue;
		}
		first.incoming.rewatch = true;
		if ((row.incoming.watched_at ?? '') > (first.incoming.watched_at ?? '')) {
			first.incoming.watched_at = row.incoming.watched_at;
		}
		gone.add(row);
	}

	const shows = new Map<string, T[]>();
	for (const row of rows) {
		if (!row.incoming.netflix || row.chosen?.kind !== 'tv' || gone.has(row)) continue;
		const key = titleKey(row.chosen);
		shows.set(key, [...(shows.get(key) ?? []), row]);
	}

	const idle = new Date(now.getTime() - 182 * DAY_MS).toISOString();

	for (const list of shows.values()) {
		const seasons = list.find((r) => r.match?.seasons?.length)?.match?.seasons ?? [];
		const regular = seasons.filter((s) => s.number >= 1);

		// Titles matched one by one fold into the show's unnamed season.
		let host = list.find(
			(r) => !r.incoming.netflix!.loose && r.incoming.netflix!.seasonKind === 'none'
		);
		for (const row of list.filter((r) => r.incoming.netflix!.loose)) {
			if (!host) {
				host = row;
				promoted.add(row);
				const meta = row.incoming.netflix!;
				meta.loose = false;
				row.incoming.title = row.chosen!.name;
				row.incoming.progress = meta.single ? 0 : 1;
				row.incoming.rewatch = false;
				row.incoming.season = null;
				row.incoming.warning = CHOOSE;
				continue;
			}
			if (host === row) continue;
			const from = row.incoming;
			// Several titles now: the row stands for the show, not one of them.
			if (promoted.has(host) && !host.incoming.netflix!.single) {
				host.incoming.label = host.incoming.netflix!.show;
			}
			host.incoming.progress = (host.incoming.progress ?? 0) + (from.netflix!.single ? 0 : 1);
			if ((from.watched_at ?? '') > (host.incoming.watched_at ?? ''))
				host.incoming.watched_at = from.watched_at;
			host.incoming.episodes = [...(host.incoming.episodes ?? []), ...(from.episodes ?? [])];
			host.incoming.watches = [...(host.incoming.watches ?? []), ...(from.watches ?? [])];
			gone.add(row);
		}
		const kept = list.filter((r) => !gone.has(r));
		const split: T[] = [];
		const numberedSeasons = new Set(
			kept.filter((r) => r.incoming.netflix!.seasonKind === 'number').map((r) => r.incoming.season)
		);

		for (const row of kept) {
			const inc = row.incoming;
			const meta = inc.netflix!;
			if (!regular.length) continue;

			// TMDB found the episodes by name: that settles the season, and
			// exactly which episodes, with no guessing. Episodes from several
			// seasons become a row each.
			if (inc.watches?.length) {
				const total = inc.episodes?.length ?? inc.watches.length;
				const missing = total - inc.watches.length;
				const bySeason = new Map<number, NonNullable<Incoming['watches']>>();
				for (const w of inc.watches) bySeason.set(w.season, [...(bySeason.get(w.season) ?? []), w]);
				const order = [...bySeason.keys()].sort((a, b) => a - b);
				const main = order.reduce(
					(m, s) => (bySeason.get(s)!.length > bySeason.get(m)!.length ? s : m),
					order[0]
				);
				const size = regular.find((s) => s.number === main)?.episodes ?? 0;
				// Netflix numbered this season and the names agree.
				const numberStands = meta.seasonKind === 'number' && inc.season === main;

				// Names TMDB writes differently ("JoJo" is translated twice): those
				// episodes go next to the ones found, in the season most of them
				// are in. The gaps between found episodes first, then after the
				// last, then before the first: Bleach's three arcs share one
				// TMDB season and must not all count from its start. When the
				// gaps are exactly as many as the episodes not found, they can
				// only be those, and nothing is in doubt. A numbered season is
				// watched from its start, so there the rest count from episode 1
				// and need no second look, undated like any other count.
				let doubt: string | undefined;
				if (missing > 0) {
					const list = bySeason.get(main)!;
					const have = new Set(list.map((w) => w.episode));
					const lo = Math.min(...have);
					const hi = Math.max(...have);
					const gaps: number[] = [];
					for (let n = lo + 1; n < hi; n++) if (!have.has(n)) gaps.push(n);
					const around = [...gaps];
					if (numberStands) for (let n = 1; n < lo; n++) around.push(n);
					for (let n = hi + 1; n <= size; n++) around.push(n);
					if (!numberStands) for (let n = lo - 1; n >= 1; n--) around.push(n);
					const fill = around.slice(0, missing);
					list.push(...fill.map((episode) => ({ season: main, episode, watched_at: null })));
					// Also certain: everything from episode 1 on, unbroken, as when a
					// whole season was watched.
					const ticked = list.map((w) => w.episode);
					const fromStart = Math.min(...ticked) === 1 && Math.max(...ticked) === ticked.length;
					if (
						!numberStands &&
						(fill.length !== missing || (gaps.length !== missing && !fromStart))
					) {
						doubt = `${missing} of ${total} episodes not found by name; counted next to the ones that were`;
					}
				}
				const over = numberStands ? tooMany(total, main, size) : {};
				doubt ??= over.warning;

				order.forEach((s, i) => {
					const target =
						i === 0 ? row : ({ ...row, incoming: { ...inc, key: `${inc.key}-s${s}` } } as T);
					const t = target.incoming;
					t.watches = bySeason.get(s);
					t.season = s;
					t.progress = t.watches!.length;
					t.episode_offset = 0;
					t.warning = s === main ? doubt : undefined;
					if (numberStands) t.info = over.info;
					else if (meta.seasonName) {
						t.info = `Netflix calls this “${meta.seasonName}”; season ${s} by its episode names`;
					} else t.info = `Season ${s}, by its episode names`;
					if (i > 0) {
						split.push(target);
						extra.set(row, [...(extra.get(row) ?? []), target]);
					}
				});
				continue;
			}

			const asSeason =
				norm(row.chosen!.name) === norm(meta.show)
					? null
					: seasonByName(meta.show, regular, row.chosen!.name);

			if (meta.seasonKind === 'named') {
				const hit =
					seasonByName(meta.seasonName ?? '', regular) ??
					(regular.length === 1 ? regular[0] : null);
				if (hit) {
					inc.season = hit.number;
					inc.warning = undefined;
					// A season of that name is in this show: it is the one meant.
					if (row.match?.note === 'Closest match by name') row.match.note = null;
					inc.info = `Netflix calls this “${meta.seasonName}”; season ${hit.number} on TMDB`;
				}
			} else if (meta.seasonKind === 'none') {
				if (inc.progress === 0) {
					// A show's own name with no episode: nothing to tick.
					inc.warning = undefined;
					inc.info = 'Watched with no episode named';
				} else if (regular.length === 1) {
					inc.season = regular[0].number;
					inc.warning = undefined;
				} else if (asSeason) {
					// Netflix's own show is one season of TMDB's: "Beyblade Burst
					// Turbo" in "Beyblade Burst", "Pokémon Journeys" in "Pokémon".
					inc.season = asSeason.number;
					inc.warning = undefined;
					if (row.match?.note === 'Closest match by name') row.match.note = null;
					inc.info = `Netflix calls this “${meta.show}”; season ${asSeason.number} on TMDB`;
				} else if (numberedSeasons.size && !numberedSeasons.has(1)) {
					// Netflix leaves the first season unnamed once a second exists.
					inc.season = 1;
					inc.warning = 'Netflix gives no season here; counted as season 1';
				}
			} else if (meta.seasonKind === 'part') {
				const byName = seasonByName(meta.seasonName ?? '', regular);
				if (byName) {
					inc.season = byName.number;
					inc.info = `Netflix calls this ${meta.seasonName}; season ${byName.number} on TMDB`;
				} else {
					inc.season = meta.part ?? null;
					inc.warning = `Netflix calls this ${meta.seasonName}; check the season`;
				}
			}

			// "Episode 7" to "Episode 12": exactly those, not the first six.
			const season = regular.find((s) => s.number === inc.season);
			if (season && meta.numbers) {
				const [lo, hi] = meta.numbers;
				// Past the season's length, the numbers count from the show's
				// first episode; TMDB numbers One Piece the same way (season 22
				// is 1089 to 1155), so ticking from there is exact. A show that
				// counts each season from 1 gets nothing ticked, never the wrong
				// episodes, and the row stays flagged.
				const absolute = hi > season.episodes;
				if (absolute || hi - lo + 1 === inc.progress) inc.episode_offset = lo - 1;
				if (absolute) inc.warning = `Netflix numbers these ${lo}–${hi}; check the episodes`;
				// The number says which episode it is, so each lands on its own day.
				inc.watches = meta.numbered?.map((episode, i) => ({
					season: season.number,
					episode,
					watched_at: watchedOn(meta.days?.[i])
				}));
			} else if (season) {
				const { warning, info } = tooMany(inc.progress ?? 0, season.number, season.episodes);
				if (warning) inc.warning = warning;
				if (info) inc.info = info;
			}

			// Matched on a shorter name than the show's own ("Monster" for
			// "Monster: The Lizzie Borden Story"): worth a look.
			const searched = row.match?.searched;
			if (
				searched &&
				!inc.warning &&
				norm(row.chosen!.name) !== norm(meta.show) &&
				norm(row.chosen!.name) !== norm(searchName(meta.show).name)
			) {
				inc.warning = `Found by searching “${searched}”; check the match`;
			}
		}

		// Completed only when every season was seen to its last episode.
		const all = [...kept, ...split];
		const complete =
			regular.length > 0 &&
			regular.every((s) =>
				all.some(
					(r) =>
						r.incoming.season === s.number &&
						(r.incoming.episode_offset ?? 0) === 0 &&
						(r.incoming.progress ?? 0) >= s.episodes
				)
			);
		const last = all.reduce(
			(m, r) => ((r.incoming.watched_at ?? '') > m ? r.incoming.watched_at! : m),
			''
		);
		const status: Status = complete ? 'completed' : last && last < idle ? 'paused' : 'watching';
		for (const row of all) row.incoming.status = status;
	}

	return rows.filter((r) => !gone.has(r)).flatMap((r) => [r, ...(extra.get(r) ?? [])]);
}

/** Words that come and go between Netflix's name and TMDB's for the same title. */
const FILLER = new Set(['the', 'a', 'movie', 'film', 'special']);

/** The same title written differently: punctuation, accents, "&", "The Movie". */
function sameName(a: string, b: string): boolean {
	const key = (v: string) =>
		v
			.replace(CIRCLED, '')
			.normalize('NFKD')
			.replace(ACCENTS, '')
			.toLowerCase()
			.replace(/&/g, ' and ')
			.split(/[^a-z0-9]+/)
			.filter((w) => w && !FILLER.has(w))
			.join('');
	const k = key(a);
	return k !== '' && k === key(b);
}
