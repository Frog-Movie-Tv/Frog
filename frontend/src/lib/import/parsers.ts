import { gunzipSync, strFromU8, unzipSync } from 'fflate';
import type { Status } from '#lib/types';
import { readMalXml } from './anime';
import { csvRecords, newRow, pick, toDate, toYear, type Incoming } from './common';
import { netflixRows, readNetflix, type NetflixExport } from './netflix';

/**
 * Readers for the exports other trackers hand out, one per format, and the
 * step that reads an uploaded file with the right one.
 */

export { animeTitle, type AnimeTitle } from './anime';
export { csvRecords, parseCsv, type Incoming } from './common';

export interface SourceFormat {
	id: string;
	label: string;
	hint: string;
	accept: string;
}

export const SOURCES: SourceFormat[] = [
	{
		id: 'mal',
		label: 'MyAnimeList',
		hint: 'The XML export from Profile → Export, zipped or not.',
		accept: '.xml,.gz'
	},
	{
		id: 'letterboxd',
		label: 'Letterboxd',
		hint: 'The ZIP from Settings → Import & Export, or a single CSV from it.',
		accept: '.zip,.csv'
	},
	{
		id: 'imdb',
		label: 'IMDb',
		hint: 'A ratings or watchlist CSV.',
		accept: '.csv'
	},
	{
		id: 'trakt',
		label: 'Trakt',
		hint: 'A JSON export of history, ratings or watchlist.',
		accept: '.json'
	},
	{
		id: 'tvtime',
		label: 'TV Time',
		hint: 'The CSV files from a GDPR data request.',
		accept: '.csv,.zip'
	},
	{
		id: 'netflix',
		label: 'Netflix',
		hint: 'Viewing activity from your Netflix profile, or the ZIP from a data request.',
		accept: '.csv,.zip'
	},
	{
		id: 'frog',
		label: 'Frog backup',
		hint: 'A frog-library.json written by this app.',
		accept: '.json'
	}
];

/** Letterboxd rates out of five, in half steps. */
function letterboxdRating(value: string): number | null {
	const n = Number(value);
	return Number.isFinite(n) && n > 0 ? Math.round((n / 5) * 100) : null;
}

function readLetterboxdCsv(name: string, text: string): Incoming[] {
	const file = name.toLowerCase();
	const records = csvRecords(text);

	return records
		.map((r) => {
			const title = pick(r, 'name', 'title');
			if (!title) return null;

			const row = newRow(title, `Letterboxd ${file}`);
			row.kind = 'movie';
			row.year = toYear(pick(r, 'year'));
			row.rating = letterboxdRating(pick(r, 'rating'));
			row.rewatch = pick(r, 'rewatch').toLowerCase() === 'yes';
			row.watched_at = toDate(pick(r, 'watched date', 'date'));

			if (file.includes('watchlist')) {
				row.status = 'planned';
				row.watched_at = null;
			} else if (file.includes('diary') || file.includes('watched')) {
				row.status = 'completed';
			} else if (file.includes('rating')) {
				row.status = 'completed';
			} else if (file.includes('like')) {
				row.favourite = true;
				row.status = 'completed';
			}

			return row;
		})
		.filter((r): r is Incoming => r !== null);
}

/** IMDb exports rate out of ten and name the type in a column. */
function readImdbCsv(text: string): Incoming[] {
	return csvRecords(text)
		.map((r) => {
			const title = pick(r, 'title', 'original title', 'primary title');
			if (!title) return null;

			const row = newRow(title, 'IMDb');
			const kind = pick(r, 'title type', 'titletype').toLowerCase();
			row.kind = kind.includes('series') || kind.includes('episode') ? 'tv' : 'movie';
			row.imdb_id = pick(r, 'const', 'imdb id', 'tconst') || null;
			row.year = toYear(pick(r, 'year', 'release date', 'start year'));

			const rated = Number(pick(r, 'your rating'));
			row.rating = Number.isFinite(rated) && rated > 0 ? rated * 10 : null;
			row.watched_at = toDate(pick(r, 'date rated'));
			row.status = row.rating !== null ? 'completed' : 'planned';

			return row;
		})
		.filter((r): r is Incoming => r !== null);
}

/** Trakt rates out of ten and carries ids, which makes matching exact. */
function readTraktJson(text: string): Incoming[] {
	let data: unknown;
	try {
		data = JSON.parse(text);
	} catch {
		throw new Error('That file is not valid JSON.');
	}

	const list = Array.isArray(data)
		? data
		: Array.isArray((data as { items?: unknown[] })?.items)
			? (data as { items: unknown[] }).items
			: [];

	if (!list.length) throw new Error('No entries found in that Trakt file.');

	return list
		.map((raw) => {
			const entry = raw as Record<string, unknown>;
			const media = (entry.movie ?? entry.show ?? entry) as Record<string, unknown> | undefined;
			const title = typeof media?.title === 'string' ? media.title : '';
			if (!title) return null;

			const row = newRow(title, 'Trakt');
			row.kind = entry.movie ? 'movie' : entry.show ? 'tv' : null;
			row.year = typeof media?.year === 'number' ? media.year : null;

			const ids = media?.ids as Record<string, unknown> | undefined;
			if (typeof ids?.tmdb === 'number') row.tmdb_id = ids.tmdb;
			if (typeof ids?.imdb === 'string') row.imdb_id = ids.imdb;

			// An entry about one episode or season says nothing of the show as a
			// whole: not its rating, and not that all of it was seen.
			const part = entry.episode || entry.season;
			const rating = entry.rating;
			if (typeof rating === 'number' && rating > 0 && !part) row.rating = rating * 10;

			const watched = entry.watched_at ?? entry.last_watched_at ?? entry.rated_at;
			if (typeof watched === 'string') row.watched_at = toDate(watched);
			if (part) row.status = 'watching';
			else row.status = row.watched_at || row.rating !== null ? 'completed' : 'planned';

			return row;
		})
		.filter((r): r is Incoming => r !== null);
}

/**
 * TV Time's GDPR export is not one documented format, so this reads any CSV
 * that has something recognisable as a show name and treats the rest as
 * optional.
 */
function readTvTimeCsv(name: string, text: string): Incoming[] {
	const records = csvRecords(text);
	if (!records.length) return [];

	const seen = new Map<string, Incoming>();
	for (const r of records) {
		const title = pick(
			r,
			'show name',
			'series name',
			'tv_show_name',
			'show_name',
			'name',
			'title',
			'movie name'
		);
		if (!title) continue;

		// One row per episode in these files; the library wants one per show.
		if (seen.has(title)) continue;

		const row = newRow(title, `TV Time ${name}`);
		const movieish = name.toLowerCase().includes('movie');
		row.kind = movieish ? 'movie' : 'tv';
		row.year = toYear(pick(r, 'year', 'first_aired', 'release_date'));
		row.watched_at = toDate(pick(r, 'watched_at', 'created_at', 'updated_at', 'date'));
		row.status = 'watching';
		seen.set(title, row);
	}

	return [...seen.values()];
}

/**
 * Frog's own backup: ids are exact, so nothing needs guessing. Version 2
 * lists every provider id (`ids`); version 1 wrote an anime TMDB lacks as
 * its AniList id made negative. The Frog id a backup carries is left out on
 * purpose: it means nothing to another Frog.
 */
function readFrogJson(text: string): Incoming[] {
	let data: { format?: string; titles?: unknown[] };
	try {
		data = JSON.parse(text);
	} catch {
		throw new Error('That file is not valid JSON.');
	}

	if (data.format !== 'frog-library' || !Array.isArray(data.titles)) {
		throw new Error('That is not a Frog backup.');
	}

	return data.titles.map((raw) => {
		const t = raw as Record<string, unknown>;
		const row = newRow(String(t.name ?? 'Unknown'), 'Frog backup');
		row.kind = t.kind === 'tv' ? 'tv' : 'movie';
		const ids = (t.ids ?? {}) as { anilist?: unknown[]; imdb?: unknown[] };
		const tmdb = typeof t.tmdb_id === 'number' ? t.tmdb_id : null;
		row.tmdb_id = tmdb !== null && tmdb > 0 ? tmdb : null;
		const anilist = Array.isArray(ids.anilist) ? ids.anilist[0] : null;
		row.anilist_id =
			typeof anilist === 'number' ? anilist : tmdb !== null && tmdb < 0 ? -tmdb : null;
		const imdb = Array.isArray(ids.imdb) ? ids.imdb[0] : null;
		row.imdb_id = typeof imdb === 'string' ? imdb : null;
		row.year = typeof t.release_date === 'string' ? toYear(t.release_date) : null;
		row.rating = typeof t.rating === 'number' ? t.rating : null;
		row.status = (t.status as Status) ?? null;
		row.favourite = t.favourite === true;

		const watches = t.watches as { watched_at?: string }[] | undefined;
		row.watched_at = watches?.[0]?.watched_at ?? null;

		// The backup says which episodes, not when: they come back undated.
		const episodes = (t.episodes ?? []) as { season: number; episode: number; watched?: boolean }[];
		const seen = episodes.filter((e) => e.watched);
		if (seen.length) {
			row.watches = seen.map((e) => ({ season: e.season, episode: e.episode, watched_at: null }));
			row.progress = seen.length;
		}
		return row;
	});
}

export interface ParsedFile {
	name: string;
	rows: Incoming[];
}

/** Read one uploaded file, unzipping it first when it is an archive. */
export async function readFile(format: string, file: File): Promise<ParsedFile[]> {
	const name = file.name.toLowerCase();

	if (name.endsWith('.gz')) {
		const bytes = gunzipSync(new Uint8Array(await file.arrayBuffer()));
		return [{ name: file.name, rows: readOne(format, file.name, strFromU8(bytes)) }];
	}

	if (name.endsWith('.zip')) {
		const buffer = new Uint8Array(await file.arrayBuffer());
		const entries = unzipSync(buffer);
		const out: ParsedFile[] = [];

		for (const [path, bytes] of Object.entries(entries)) {
			const wanted = format === 'mal' ? '.xml' : '.csv';
			if (!path.toLowerCase().endsWith(wanted)) continue;
			// Letterboxd also ships comments and lists, which are not a library.
			if (/comment|review|lists?\//i.test(path)) continue;
			const rows = readOne(format, path, strFromU8(bytes));
			if (rows.length) out.push({ name: path, rows });
		}

		if (!out.length) throw new Error('No usable CSV files inside that archive.');
		return out;
	}

	const text = await file.text();
	return [{ name: file.name, rows: readOne(format, file.name, text) }];
}

/**
 * A Netflix file, read but not yet grouped: the personal-data ZIP can hold
 * several profiles, and the person chooses whose plays to import first.
 */
export async function readNetflixFile(file: File): Promise<NetflixExport> {
	if (!file.name.toLowerCase().endsWith('.zip')) return readNetflix(await file.text());

	const entries = unzipSync(new Uint8Array(await file.arrayBuffer()));
	// CONTENT_INTERACTION/ViewingActivity.csv in a data request; a zipped
	// NetflixViewingHistory.csv works too.
	const path =
		Object.keys(entries).find((p) => /(^|\/)viewingactivity\.csv$/i.test(p)) ??
		Object.keys(entries).find((p) => /viewinghistory[^/]*\.csv$/i.test(p));
	if (!path) throw new Error('No viewing activity inside that archive.');
	return readNetflix(strFromU8(entries[path]));
}

function readOne(format: string, name: string, text: string): Incoming[] {
	switch (format) {
		case 'netflix':
			return netflixRows(readNetflix(text)).rows;
		case 'letterboxd':
			return readLetterboxdCsv(name, text);
		case 'imdb':
			return readImdbCsv(text);
		case 'trakt':
			return readTraktJson(text);
		case 'tvtime':
			return readTvTimeCsv(name, text);
		case 'frog':
			return readFrogJson(text);
		case 'mal':
			return readMalXml(text);
		default:
			throw new Error('Unknown format.');
	}
}

/**
 * Collapse rows that mean the same title.
 *
 * A Letterboxd export lists a film in diary, ratings and likes; importing
 * three copies of it would be wrong. The richest row wins, and a like or a
 * rating found on a later row is folded into it.
 */
export function dedupe(rows: Incoming[]): Incoming[] {
	const byTitle = new Map<string, Incoming>();

	for (const row of rows) {
		const id = row.tmdb_id
			? `t${row.tmdb_id}`
			: row.imdb_id
				? `i${row.imdb_id}`
				: `${row.title.toLowerCase()}|${row.year ?? ''}|${row.season ?? ''}|${row.episode_offset ?? 0}`;

		const existing = byTitle.get(id);
		if (!existing) {
			byTitle.set(id, { ...row });
			continue;
		}

		existing.rating ??= row.rating;
		existing.imdb_id ??= row.imdb_id;
		existing.tmdb_id ??= row.tmdb_id;
		existing.year ??= row.year;
		existing.favourite ||= row.favourite;
		existing.rewatch ||= row.rewatch;
		// Prefer a real viewing date over nothing, and the earliest one.
		if (row.watched_at && (!existing.watched_at || row.watched_at < existing.watched_at)) {
			existing.watched_at = row.watched_at;
		}
		// Watched beats planned.
		if (row.status === 'completed') existing.status = 'completed';
	}

	return [...byTitle.values()];
}
