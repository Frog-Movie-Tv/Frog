import { unzipSync, strFromU8 } from 'fflate';
import type { Status } from '#lib/types';

/**
 * Readers for the exports other trackers hand out.
 *
 * Everything lands in one shape, `Incoming`, so the matching step and the
 * wizard never need to know where a row came from. Ratings are rescaled to
 * 0..100 here, which is the only scale the rest of Frog uses.
 */
export interface Incoming {
	key: string;
	title: string;
	year: number | null;
	imdb_id: string | null;
	tmdb_id: number | null;
	kind: 'movie' | 'tv' | null;
	rating: number | null;
	status: Status | null;
	watched_at: string | null;
	favourite: boolean;
	rewatch: boolean;
	/** Which file inside the export this came from, shown in the preview. */
	source: string;
}

export interface SourceFormat {
	id: string;
	label: string;
	hint: string;
	accept: string;
}

export const SOURCES: SourceFormat[] = [
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
		id: 'frog',
		label: 'Frog backup',
		hint: 'A frog-library.json written by this app.',
		accept: '.json'
	}
];

/** Split CSV text into rows, honouring quoted fields and embedded newlines. */
export function parseCsv(text: string): string[][] {
	const rows: string[][] = [];
	let row: string[] = [];
	let field = '';
	let quoted = false;

	// Strip a byte order mark, which Excel-written exports love to include.
	const input = text.charCodeAt(0) === 0xfeff ? text.slice(1) : text;

	for (let i = 0; i < input.length; i++) {
		const c = input[i];

		if (quoted) {
			if (c === '"') {
				if (input[i + 1] === '"') {
					field += '"';
					i++;
				} else {
					quoted = false;
				}
			} else {
				field += c;
			}
			continue;
		}

		if (c === '"') quoted = true;
		else if (c === ',') {
			row.push(field);
			field = '';
		} else if (c === '\n') {
			row.push(field);
			rows.push(row);
			row = [];
			field = '';
		} else if (c !== '\r') {
			field += c;
		}
	}

	if (field || row.length) {
		row.push(field);
		rows.push(row);
	}

	return rows.filter((r) => r.some((cell) => cell.trim() !== ''));
}

/** Rows as objects keyed by header, with headers lowercased and trimmed. */
export function csvRecords(text: string): Record<string, string>[] {
	const rows = parseCsv(text);
	if (rows.length < 2) return [];
	const headers = rows[0].map((h) => h.trim().toLowerCase());
	return rows.slice(1).map((cells) => {
		const record: Record<string, string> = {};
		headers.forEach((h, i) => (record[h] = (cells[i] ?? '').trim()));
		return record;
	});
}

function pick(record: Record<string, string>, ...names: string[]): string {
	for (const name of names) {
		const value = record[name];
		if (value) return value;
	}
	return '';
}

function toYear(value: string): number | null {
	const match = value.match(/\d{4}/);
	return match ? Number(match[0]) : null;
}

/** Letterboxd dates are already yyyy-mm-dd; everything else gets normalised. */
function toDate(value: string): string | null {
	if (!value) return null;
	const iso = value.match(/^(\d{4})-(\d{2})-(\d{2})/);
	if (iso) return `${iso[0]}T20:00:00Z`;
	const parsed = new Date(value);
	if (Number.isNaN(parsed.getTime())) return null;
	return parsed.toISOString();
}

let counter = 0;
const nextKey = () => `row-${++counter}`;

function base(title: string, source: string): Incoming {
	return {
		key: nextKey(),
		title,
		year: null,
		imdb_id: null,
		tmdb_id: null,
		kind: null,
		rating: null,
		status: null,
		watched_at: null,
		favourite: false,
		rewatch: false,
		source
	};
}

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

			const row = base(title, `Letterboxd ${file}`);
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

			const row = base(title, 'IMDb');
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

			const row = base(title, 'Trakt');
			row.kind = entry.movie ? 'movie' : entry.show ? 'tv' : null;
			row.year = typeof media?.year === 'number' ? media.year : null;

			const ids = media?.ids as Record<string, unknown> | undefined;
			if (typeof ids?.tmdb === 'number') row.tmdb_id = ids.tmdb;
			if (typeof ids?.imdb === 'string') row.imdb_id = ids.imdb;

			const rating = entry.rating;
			if (typeof rating === 'number' && rating > 0) row.rating = rating * 10;

			const watched = entry.watched_at ?? entry.last_watched_at ?? entry.rated_at;
			if (typeof watched === 'string') row.watched_at = toDate(watched);
			row.status = row.watched_at || row.rating !== null ? 'completed' : 'planned';

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

		const row = base(title, `TV Time ${name}`);
		const movieish = name.toLowerCase().includes('movie');
		row.kind = movieish ? 'movie' : 'tv';
		row.year = toYear(pick(r, 'year', 'first_aired', 'release_date'));
		row.watched_at = toDate(pick(r, 'watched_at', 'created_at', 'updated_at', 'date'));
		row.status = 'watching';
		seen.set(title, row);
	}

	return [...seen.values()];
}

/** Frog's own backup: ids are exact, so nothing needs guessing. */
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
		const row = base(String(t.name ?? 'Unknown'), 'Frog backup');
		row.kind = t.kind === 'tv' ? 'tv' : 'movie';
		row.tmdb_id = typeof t.tmdb_id === 'number' ? t.tmdb_id : null;
		row.year = typeof t.release_date === 'string' ? toYear(t.release_date) : null;
		row.rating = typeof t.rating === 'number' ? t.rating : null;
		row.status = (t.status as Status) ?? null;
		row.favourite = t.favourite === true;

		const watches = t.watches as { watched_at?: string }[] | undefined;
		row.watched_at = watches?.[0]?.watched_at ?? null;
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

	if (name.endsWith('.zip')) {
		const buffer = new Uint8Array(await file.arrayBuffer());
		const entries = unzipSync(buffer);
		const out: ParsedFile[] = [];

		for (const [path, bytes] of Object.entries(entries)) {
			if (!path.toLowerCase().endsWith('.csv')) continue;
			// Letterboxd also ships comments and lists, which are not a library.
			if (/comment|review|list\//i.test(path)) continue;
			const rows = readOne(format, path, strFromU8(bytes));
			if (rows.length) out.push({ name: path, rows });
		}

		if (!out.length) throw new Error('No usable CSV files inside that archive.');
		return out;
	}

	const text = await file.text();
	return [{ name: file.name, rows: readOne(format, file.name, text) }];
}

function readOne(format: string, name: string, text: string): Incoming[] {
	switch (format) {
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
				: `${row.title.toLowerCase()}|${row.year ?? ''}`;

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
