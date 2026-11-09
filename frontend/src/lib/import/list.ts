/**
 * A list pasted onto a shelf, as import rows in the list's own order: one
 * title a line ("Heat (1995)", "1. Dune, 2021"), a link (TMDB, IMDb,
 * Letterboxd, AniList) or an IMDb id, or a CSV with a Name or Title column,
 * as Letterboxd's list export has.
 */
import { csvRecords, newRow, pick, toYear, type Incoming } from './common';

const SOURCE = 'Pasted list';

/** The most a pasted list takes; a longer one is cut here. */
export const LIST_LIMIT = 500;

function fromLine(raw: string): Incoming | null {
	let line = raw.trim();
	if (!line) return null;
	// A list's numbering or bullets: "12.", "3)", "-", "*", "•".
	line = line.replace(/^(\d{1,4}[.)]\s+|[-*•]\s+)/, '').trim();
	if (!line) return null;

	const row = newRow(line, SOURCE, 'list');
	row.label = raw.trim();

	const imdb = line.match(/\btt\d{5,10}\b/);
	const tmdb = line.match(/themoviedb\.org\/(movie|tv)\/(\d+)/i);
	const anilist = line.match(/anilist\.co\/(anime|manga)\/(\d+)/i);
	const letterboxd = line.match(/letterboxd\.com\/(?:[^/\s]+\/)?film\/([a-z0-9-]+)/i);
	if (tmdb) {
		row.kind = tmdb[1].toLowerCase() as Incoming['kind'];
		row.tmdb_id = Number(tmdb[2]);
		row.title = '';
		return row;
	}
	if (anilist) {
		row.anilist_id = Number(anilist[2]);
		if (anilist[1].toLowerCase() === 'manga') row.kind = 'manga';
		row.title = '';
		return row;
	}
	if (imdb) {
		row.imdb_id = imdb[0];
		row.title = line
			.replace(/https?:\/\/\S+/g, '')
			.replace(imdb[0], '')
			.trim();
		return row;
	}
	if (letterboxd) {
		// A slug ends in its year when Letterboxd had two films of one name.
		// Only a year a film could have: "blade-runner-2049" is a name.
		const slug = letterboxd[1].replace(/-(\d{4})$/, (whole, y) => {
			if (Number(y) < 1880 || Number(y) > new Date().getFullYear() + 3) return whole;
			row.year = Number(y);
			return '';
		});
		row.title = slug.replace(/-/g, ' ');
		row.kind = 'movie';
		return row;
	}

	// A year at the end: "(1995)", ", 1995", " - 1995", or after a tab.
	const year = line.match(/^(.+?)(?:\s*\((\d{4})\)|\s*[,\t]\s*(\d{4})|\s+[-–]\s+(\d{4}))\s*$/);
	if (year) {
		row.title = year[1].trim();
		row.year = Number(year[2] ?? year[3] ?? year[4]);
	} else {
		row.title = line;
	}
	return row.title ? row : null;
}

/** Every title in a pasted list, in its order. */
export function parseList(text: string): Incoming[] {
	const first = text.split(/\r?\n/, 1)[0]?.toLowerCase() ?? '';
	const isCsv = /(^|,)\s*"?(name|title)"?\s*(,|$)/.test(first) && first.includes(',');
	if (isCsv) {
		return csvRecords(text)
			.map((r) => {
				const imdb = pick(r, 'imdb id', 'imdb_id', 'const');
				const url = pick(r, 'letterboxd uri', 'url');
				const name = pick(r, 'name', 'title');
				const row = fromLine(imdb || (url && !name ? url : name)) ?? null;
				if (!row) return null;
				if (name) row.title = name;
				const year = toYear(pick(r, 'year', 'release year'));
				if (year) row.year = year;
				return row;
			})
			.filter((r): r is Incoming => r !== null)
			.slice(0, LIST_LIMIT);
	}
	return text
		.split(/\r?\n/)
		.map(fromLine)
		.filter((r): r is Incoming => r !== null)
		.slice(0, LIST_LIMIT);
}
