/**
 * What every import reader shares: the row shape, reading CSV, and the small
 * conversions between a tracker's fields and Frog's.
 */
import type { Status } from '#lib/types';
import type { NetflixMeta } from './netflix';

/**
 * One row of somebody else's export, whatever it came from.
 *
 * Every reader lands in this shape, so the matching step and the wizard
 * never need to know where a row came from. Ratings are rescaled to 0..100
 * by the reader, which is the only scale the rest of Frog uses.
 */
export interface Incoming {
	key: string;
	title: string;
	year: number | null;
	imdb_id: string | null;
	tmdb_id: number | null;
	/** AniList's id, for an anime TMDB lacks (from a Frog backup). */
	anilist_id?: number | null;
	kind: 'movie' | 'tv' | null;
	rating: number | null;
	status: Status | null;
	watched_at: string | null;
	favourite: boolean;
	rewatch: boolean;
	/** Which file inside the export this came from, shown in the preview. */
	source: string;
	/** What the source called it, when `title` was cleaned up for searching. */
	label?: string;
	/** TMDB season this row covers, for anime trackers that list seasons. */
	season?: number | null;
	/** Episodes watched within that season. */
	progress?: number | null;
	/** Where this row starts inside the TMDB season (a season "part 2"). */
	episode_offset?: number;
	/** How many episodes this row covers, when known. */
	episode_total?: number | null;
	/** Other names to try when matching. */
	alt_titles?: string[];
	/** From an anime tracker: prefer animated matches. */
	anime?: boolean;
	/** Something the preview should point out before importing. */
	warning?: string;
	/** A neutral remark shown under the row, such as how a season was mapped. */
	info?: string;
	/** With no `kind`: which kind to take when the search finds both. */
	prefer?: 'movie' | 'tv';
	/** The source's name for a season it does not number, to tell same-named shows apart. */
	season_name?: string | null;
	/** Where a Netflix row came from, for the step after matching. */
	netflix?: NetflixMeta;
	/** Episode names to find on TMDB when the season is not known for sure. */
	episodes?: string[];
	/** Exactly these episodes, once found: ticked instead of `season` and `progress`. */
	watches?: { season: number; episode: number; watched_at: string | null }[];
}

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

export function pick(record: Record<string, string>, ...names: string[]): string {
	for (const name of names) {
		const value = record[name];
		if (value) return value;
	}
	return '';
}

export function toYear(value: string): number | null {
	const match = value.match(/\d{4}/);
	return match ? Number(match[0]) : null;
}

/** Letterboxd dates are already yyyy-mm-dd; everything else gets normalised. */
export function toDate(value: string): string | null {
	if (!value) return null;
	const iso = value.match(/^(\d{4})-(\d{2})-(\d{2})/);
	if (iso) return `${iso[0]}T20:00:00Z`;
	const parsed = new Date(value);
	if (Number.isNaN(parsed.getTime())) return null;
	return parsed.toISOString();
}

let counter = 0;

/** An empty row with a key no other row in this import has. */
export function newRow(title: string, source: string, prefix = 'row'): Incoming {
	return {
		key: `${prefix}-${++counter}`,
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
