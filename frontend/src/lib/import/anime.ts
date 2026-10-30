import type { Status } from '#lib/types';
import { newRow, toDate, type Incoming } from './common';

/**
 * Anime trackers (MyAnimeList's export here; AniList syncs on the server)
 * give every season its own entry. These turn such entries into rows for
 * TMDB's one show with numbered seasons.
 */

const ROMAN: Record<string, number> = { II: 2, III: 3, IV: 4, V: 5, VI: 6, VII: 7, VIII: 8 };

export interface AnimeTitle {
	/** The name with season and part markers removed, for searching TMDB. */
	base: string;
	season: number | null;
	part: number | null;
	/** "The Final Season" and the like: a season, but no number to go on. */
	unnumbered: boolean;
}

/**
 * Split an anime tracker's title into the show and the season it means.
 *
 * Trackers give every season its own entry and name it a dozen ways:
 * "Season 2", "2nd Season", "II", a trailing digit, "Part 2", "Cour 2", "The
 * Final Season". TMDB has one show with numbered seasons, so the show name is
 * what gets searched and the season number is what gets ticked.
 */
export function animeTitle(title: string): AnimeTitle {
	let rest = title.trim();
	let part: number | null = null;
	let season: number | null = null;
	let unnumbered = false;

	const partMatch = rest.match(/(?:\s*:|\s+[-–])?\s+(?:Part|Cour)\s*(\d+)\s*$/i);
	if (partMatch) {
		part = Number(partMatch[1]);
		rest = rest.slice(0, partMatch.index).trim();
	}

	const patterns: [RegExp, (m: RegExpMatchArray) => number | null][] = [
		[/(?:\s*:|\s+[-–])?\s+(?:Season|Saison)\s*(\d+)\s*$/i, (m) => Number(m[1])],
		[/(?:\s*:|\s+[-–])?\s+(\d+)(?:st|nd|rd|th)\s+Season\s*$/i, (m) => Number(m[1])],
		[/\s+(II|III|IV|V|VI|VII|VIII)\s*$/, (m) => ROMAN[m[1]]],
		[/\s+([2-9])\s*$/, (m) => Number(m[1])],
		[/(?:\s*:|\s+[-–])?\s+(?:The\s+)?Final\s+Season\s*$/i, () => null]
	];

	for (const [pattern, read] of patterns) {
		const m = rest.match(pattern);
		if (!m) continue;
		season = read(m);
		unnumbered = season === null;
		// The pattern already took the separator before the marker; anything
		// left at the end, like Re:ZERO's closing dash, belongs to the title.
		rest = rest.slice(0, m.index).trim();
		break;
	}

	return { base: rest || title.trim(), season, part, unnumbered };
}

/** Tracker status names, in either tracker's spelling. */
function animeStatus(value: string): Status {
	const v = value.toLowerCase().replace(/[\s_-]/g, '');
	if (v === 'completed') return 'completed';
	if (v === 'paused' || v === 'onhold') return 'paused';
	if (v === 'dropped') return 'dropped';
	if (v === 'planning' || v === 'plantowatch') return 'planned';
	return 'watching';
}

interface AnimeRow {
	titles: string[];
	format: string;
	episodes: number | null;
	progress: number;
	score100: number | null;
	status: Status;
	rewatching: boolean;
	finished: string | null;
	year: number | null;
	source: string;
}

/** Turn one tracker entry into a row Frog can match and import. */
function fromAnime(a: AnimeRow): Incoming {
	const display = a.titles[0];
	const parsed = a.titles.map(animeTitle);
	const main = parsed[0];
	const row = newRow(main.base, a.source);

	row.label = display;
	row.anime = true;
	row.year = a.year;
	row.status = a.status;
	row.rating = a.score100;
	row.rewatch = a.rewatching;
	row.episode_total = a.episodes;
	row.alt_titles = [
		...new Set(
			parsed
				.slice(1)
				.map((t) => t.base)
				.concat(a.titles.slice(1))
		)
	];

	const format = a.format.toUpperCase();
	if (format === 'MOVIE') {
		row.kind = 'movie';
		row.watched_at = a.status === 'completed' ? toDate(a.finished ?? '') : null;
		return row;
	}

	row.kind = 'tv';
	if (format === 'OVA' || format === 'SPECIAL') {
		// Specials live in TMDB's season 0, numbered differently: import the
		// status, leave the episodes for a person to tick.
		row.season = 0;
		row.progress = 0;
		row.warning = 'Special: matched to the show, episodes not ticked';
		return row;
	}

	row.season = main.season ?? (main.unnumbered ? null : 1);
	row.progress = a.status === 'completed' ? null : a.progress || null;
	if (main.unnumbered) row.warning = 'Choose which season this is';
	(row as Incoming & { part?: number | null }).part = main.part;
	return row;
}

/**
 * Place later parts of a season after the earlier ones.
 *
 * "Season 3 Part 2" is the second half of TMDB's season 3, so its first
 * episode is the part before's episode count plus one.
 */
function applyPartOffsets(rows: Incoming[]): Incoming[] {
	type WithPart = Incoming & { part?: number | null };
	const groups = new Map<string, WithPart[]>();
	for (const row of rows as WithPart[]) {
		if (row.kind !== 'tv' || row.season == null) continue;
		const key = `${row.title.toLowerCase()}|${row.season}`;
		groups.set(key, [...(groups.get(key) ?? []), row]);
	}

	for (const group of groups.values()) {
		if (group.length < 2) continue;
		group.sort((a, b) => (a.part ?? 1) - (b.part ?? 1));
		let offset = 0;
		let known = true;
		for (const row of group) {
			row.episode_offset = offset;
			if (!known) row.warning = 'Could not place this part; check the episodes';
			if (row.episode_total == null) known = false;
			offset += row.episode_total ?? 0;
		}
	}
	for (const row of rows as WithPart[]) delete row.part;
	return rows;
}

/** MyAnimeList's XML export: one <anime> element per entry. */
export function readMalXml(text: string): Incoming[] {
	const doc = new DOMParser().parseFromString(text, 'application/xml');
	if (doc.querySelector('parsererror')) throw new Error('That file is not a MyAnimeList export.');

	const get = (el: Element, tag: string) =>
		el.getElementsByTagName(tag)[0]?.textContent?.trim() ?? '';
	const anime = Array.from(doc.getElementsByTagName('anime'));
	if (!anime.length) throw new Error('No anime found in that file.');

	const rows = anime.map((el) => {
		const score = Number(get(el, 'my_score'));
		const finished = get(el, 'my_finish_date');
		const episodes = Number(get(el, 'series_episodes'));
		return fromAnime({
			titles: [get(el, 'series_title')],
			format: get(el, 'series_type') || 'TV',
			episodes: episodes > 0 ? episodes : null,
			progress: Number(get(el, 'my_watched_episodes')) || 0,
			score100: score > 0 ? score * 10 : null,
			status: animeStatus(get(el, 'my_status')),
			rewatching: get(el, 'my_rewatching') === '1',
			finished: finished && !finished.startsWith('0000') ? finished : null,
			year: null,
			source: 'MyAnimeList'
		});
	});
	return applyPartOffsets(rows);
}
