/**
 * Stats as a dashboard: click a bar and every other chart narrows to the
 * titles under it. The server sends one row per title (`StatsFact`); this
 * works the charts out from the rows a selection keeps.
 *
 * Each chart is drawn from every selection but its own, so its other bars
 * stay to be picked, with the chosen one marked: the way Qlik Sense and its
 * kind behave.
 */
import type { Kind, StatsTitle } from './types';

/** [unit, minutes, films, episodes, rewatches]; unit '' for undated. */
export type FactUnit = [string, number, number, number, number];

export interface StatsFact {
	id: number;
	kind: Kind;
	name: string;
	poster_path: string | null;
	genres: string[];
	year: number | null;
	/** 0-100 */
	rating: number | null;
	units: FactUnit[];
}

export interface StatsFilter {
	kind?: 'movie' | 'tv';
	genre?: string;
	decade?: number;
	/** A ratings bar, 0-9: the score out of ten, less one. */
	rating?: number;
	/** A month (YYYY-MM) or a year (YYYY). */
	unit?: string;
}

export type Dimension = keyof StatsFilter;

/** The bar a rating sits in: rounded to the nearest of ten, as the server does. */
export const bucketOf = (rating: number) => Math.min(Math.max(Math.round(rating / 10), 1), 10) - 1;

export const decadeOf = (year: number) => Math.floor(year / 10) * 10;

/** What a title counts of itself under a selection. */
function counted(f: StatsFact, filter: StatsFilter, skip?: Dimension): FactUnit[] {
	if (filter.unit === undefined || skip === 'unit') return f.units;
	return f.units.filter((u) => u[0] === filter.unit);
}

function keeps(f: StatsFact, filter: StatsFilter, skip?: Dimension): boolean {
	if (skip !== 'kind' && filter.kind && f.kind !== filter.kind) return false;
	if (skip !== 'genre' && filter.genre !== undefined && !f.genres.includes(filter.genre)) {
		return false;
	}
	if (skip !== 'decade' && filter.decade !== undefined) {
		if (f.year === null || decadeOf(f.year) !== filter.decade) return false;
	}
	if (skip !== 'rating' && filter.rating !== undefined) {
		if (f.rating === null || bucketOf(f.rating) !== filter.rating) return false;
	}
	if (skip !== 'unit' && filter.unit !== undefined && !counted(f, filter).length) return false;
	return true;
}

const sum = (units: FactUnit[], i: 1 | 2 | 3 | 4) => units.reduce((n, u) => n + u[i], 0);

export interface StatsView {
	totals: {
		minutes: number;
		films: number;
		episodes: number;
		shows: number;
		titles: number;
		rewatches: number;
		mean_rating: number | null;
		rated: number;
	};
	timeline: { unit: string; minutes: number; films: number; episodes: number }[];
	genres: { name: string; titles: number; minutes: number }[];
	decades: { decade: number; titles: number }[];
	ratings: number[];
	top_shows: StatsTitle[];
	top_films: StatsTitle[];
}

/** Every chart under a selection. `units` is the timeline's own bars, in order. */
export function view(facts: StatsFact[], filter: StatsFilter, units: string[]): StatsView {
	const kept = facts.filter((f) => keeps(f, filter));
	const watched = kept.filter((f) => counted(f, filter).length > 0);

	let minutes = 0;
	let films = 0;
	let episodes = 0;
	let rewatches = 0;
	let shows = 0;
	for (const f of watched) {
		const u = counted(f, filter);
		minutes += sum(u, 1);
		films += sum(u, 2);
		episodes += sum(u, 3);
		rewatches += sum(u, 4);
		if (sum(u, 3) > 0) shows++;
	}
	const ratedTitles = kept.filter((f) => f.rating !== null);
	const mean = ratedTitles.length
		? ratedTitles.reduce((n, f) => n + (f.rating ?? 0), 0) / ratedTitles.length
		: null;

	// The timeline: every selection but its own.
	const byUnit = new Map(units.map((u) => [u, { unit: u, minutes: 0, films: 0, episodes: 0 }]));
	for (const f of facts.filter((f) => keeps(f, filter, 'unit'))) {
		for (const [unit, m, fl, ep] of f.units) {
			const point = byUnit.get(unit);
			if (!point) continue;
			point.minutes += m;
			point.films += fl;
			point.episodes += ep;
		}
	}

	const genreMap = new Map<string, { name: string; titles: number; minutes: number }>();
	for (const f of facts.filter((f) => keeps(f, filter, 'genre'))) {
		const u = counted(f, filter, 'genre');
		if (!u.length) continue;
		for (const g of f.genres) {
			const row = genreMap.get(g) ?? { name: g, titles: 0, minutes: 0 };
			row.titles++;
			row.minutes += sum(u, 1);
			genreMap.set(g, row);
		}
	}
	const genres = [...genreMap.values()]
		.sort((a, b) => b.titles - a.titles || b.minutes - a.minutes)
		.slice(0, 12);

	const decadeMap = new Map<number, number>();
	for (const f of facts.filter((f) => keeps(f, filter, 'decade'))) {
		if (f.year === null || !counted(f, filter, 'decade').length) continue;
		const d = decadeOf(f.year);
		decadeMap.set(d, (decadeMap.get(d) ?? 0) + 1);
	}
	const decades = [...decadeMap.entries()]
		.sort((a, b) => a[0] - b[0])
		.map(([decade, titles]) => ({ decade, titles }));

	const ratings = Array<number>(10).fill(0);
	for (const f of facts.filter((f) => keeps(f, filter, 'rating'))) {
		if (f.rating !== null) ratings[bucketOf(f.rating)]++;
	}

	const asTop = (f: StatsFact, watches: number, mins: number): StatsTitle => ({
		id: f.id,
		kind: f.kind,
		name: f.name,
		poster_path: f.poster_path,
		watches,
		minutes: mins,
		rating: f.rating
	});
	const top_shows = watched
		.map((f) => ({ f, u: counted(f, filter) }))
		.filter(({ u }) => sum(u, 3) > 0)
		.sort((a, b) => sum(b.u, 3) - sum(a.u, 3))
		.slice(0, 6)
		.map(({ f, u }) => asTop(f, sum(u, 3), sum(u, 1)));
	const top_films = watched
		.map((f) => ({ f, u: counted(f, filter) }))
		.filter(({ u }) => sum(u, 2) > 0)
		.sort((a, b) => (b.f.rating ?? -1) - (a.f.rating ?? -1) || sum(b.u, 2) - sum(a.u, 2))
		.slice(0, 6)
		.map(({ f, u }) => asTop(f, sum(u, 2), sum(u, 1)));

	return {
		totals: {
			minutes,
			films,
			episodes,
			shows,
			titles: watched.length,
			rewatches,
			mean_rating: mean,
			rated: ratedTitles.length
		},
		timeline: [...byUnit.values()],
		genres,
		decades,
		ratings,
		top_shows,
		top_films
	};
}

/** Pick a bar, or put it back when it is the one picked. */
export function toggle<K extends Dimension>(
	filter: StatsFilter,
	key: K,
	value: StatsFilter[K]
): StatsFilter {
	const next = { ...filter };
	if (next[key] === value) delete next[key];
	else next[key] = value;
	return next;
}

/* --- Manga ------------------------------------------------------------------ */

/** [unit, chapters read in it] */
export type MangaUnit = [string, number];

/** One manga in the library, as the manga half of Stats sends it. */
export interface MangaFact {
	id: number;
	name: string;
	poster_path: string | null;
	genres: string[];
	status: string;
	/** 0-100 */
	rating: number | null;
	/** How far in, as a count: what all time counts. */
	progress: number;
	units: MangaUnit[];
}

export interface MangaFilter {
	genre?: string;
	rating?: number;
	status?: string;
	unit?: string;
}

export type MangaDimension = keyof MangaFilter;

function mangaKeeps(f: MangaFact, filter: MangaFilter, skip?: MangaDimension): boolean {
	if (skip !== 'genre' && filter.genre !== undefined && !f.genres.includes(filter.genre)) {
		return false;
	}
	if (skip !== 'rating' && filter.rating !== undefined) {
		if (f.rating === null || bucketOf(f.rating) !== filter.rating) return false;
	}
	if (skip !== 'status' && filter.status !== undefined && f.status !== filter.status) return false;
	if (skip !== 'unit' && filter.unit !== undefined && !f.units.some((u) => u[0] === filter.unit)) {
		return false;
	}
	return true;
}

/** Chapters a manga counts under a selection: those of the bar picked, else all it has. */
function chaptersOf(f: MangaFact, filter: MangaFilter, inYear: boolean): number {
	if (filter.unit !== undefined) {
		return f.units.filter((u) => u[0] === filter.unit).reduce((n, u) => n + u[1], 0);
	}
	// In a year, what was read in it; over all time, how far in.
	return inYear ? f.units.reduce((n, u) => n + u[1], 0) : f.progress;
}

export interface MangaView {
	totals: {
		manga: number;
		chapters: number;
		completed: number;
		mean_rating: number | null;
		rated: number;
	};
	timeline: { unit: string; chapters: number }[];
	genres: { name: string; titles: number }[];
	ratings: number[];
	by_status: Record<string, number>;
	top: {
		id: number;
		kind: 'manga';
		name: string;
		poster_path: string | null;
		chapters: number;
		rating: number | null;
	}[];
}

/** Every manga chart under a selection, each from every pick but its own. */
export function mangaView(
	facts: MangaFact[],
	filter: MangaFilter,
	units: string[],
	inYear: boolean
): MangaView {
	const kept = facts.filter((f) => mangaKeeps(f, filter));
	const rated = kept.filter((f) => f.rating !== null);

	const byUnit = new Map(units.map((u) => [u, { unit: u, chapters: 0 }]));
	for (const f of facts.filter((f) => mangaKeeps(f, filter, 'unit'))) {
		for (const [unit, n] of f.units) {
			const point = byUnit.get(unit);
			if (point) point.chapters += n;
		}
	}

	const genreMap = new Map<string, number>();
	for (const f of facts.filter((f) => mangaKeeps(f, filter, 'genre'))) {
		for (const g of f.genres) genreMap.set(g, (genreMap.get(g) ?? 0) + 1);
	}

	const ratings = Array<number>(10).fill(0);
	for (const f of facts.filter((f) => mangaKeeps(f, filter, 'rating'))) {
		if (f.rating !== null) ratings[bucketOf(f.rating)]++;
	}

	const by_status: Record<string, number> = {};
	for (const f of facts.filter((f) => mangaKeeps(f, filter, 'status'))) {
		by_status[f.status] = (by_status[f.status] ?? 0) + 1;
	}

	const top = kept
		.map((f) => ({ f, chapters: chaptersOf(f, filter, inYear) }))
		.sort(
			(a, b) =>
				Number(a.f.rating === null) - Number(b.f.rating === null) ||
				(b.f.rating ?? 0) - (a.f.rating ?? 0) ||
				b.chapters - a.chapters
		)
		.slice(0, 12)
		.map(({ f, chapters }) => ({
			id: f.id,
			kind: 'manga' as const,
			name: f.name,
			poster_path: f.poster_path,
			chapters,
			rating: f.rating
		}));

	return {
		totals: {
			manga: kept.length,
			chapters: kept.reduce((n, f) => n + chaptersOf(f, filter, inYear), 0),
			completed: kept.filter((f) => f.status === 'completed').length,
			mean_rating: rated.length
				? rated.reduce((n, f) => n + (f.rating ?? 0), 0) / rated.length
				: null,
			rated: rated.length
		},
		timeline: [...byUnit.values()],
		genres: [...genreMap.entries()]
			.map(([name, titles]) => ({ name, titles }))
			.sort((a, b) => b.titles - a.titles || a.name.localeCompare(b.name))
			.slice(0, 12),
		ratings,
		by_status,
		top
	};
}

/** Pick a manga bar, or put it back when it is the one picked. */
export function toggleManga<K extends MangaDimension>(
	filter: MangaFilter,
	key: K,
	value: MangaFilter[K]
): MangaFilter {
	const next = { ...filter };
	if (next[key] === value) delete next[key];
	else next[key] = value;
	return next;
}
