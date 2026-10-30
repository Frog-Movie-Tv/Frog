import { titleKey } from '#lib/titles';
import { bySeason, type Foldable } from './rows';

/**
 * Sorting an import's rows for review: what is the matter with each one, and
 * which rows belong to the same show.
 */

export interface ReviewRow extends Foldable {
	include: boolean;
	/** Too little of it was watched to count, whatever became of the row since. */
	short?: boolean;
	/** Left out because of that, rather than by the person. */
	held?: boolean;
}

/** The server's word for a match it found by name alone. */
export const CLOSEST = 'Closest match by name';

export type ProblemKind = 'missing' | 'closest' | 'match' | 'season' | 'episodes';

/** In the order the chips show them: what stops a title first. */
export const PROBLEM_KINDS: ProblemKind[] = ['missing', 'closest', 'match', 'season', 'episodes'];

export const PROBLEM_LABEL: Record<ProblemKind, string> = {
	missing: 'No match',
	closest: 'Closest match',
	match: 'Check the match',
	season: 'Season unclear',
	episodes: 'Episodes'
};

const ABOUT_SEASON = /which season|no season|check the season/;
const ABOUT_EPISODES = /episodes/;

function kindOf(message: string): ProblemKind {
	if (ABOUT_SEASON.test(message)) return 'season';
	if (ABOUT_EPISODES.test(message)) return 'episodes';
	return 'match';
}

/** Everything the matter with a row; none when it is ready as it stands. */
export function problemsOf(row: Foldable): ProblemKind[] {
	if (!row.chosen) return ['missing'];
	const { note } = row.match ?? {};
	const { warning } = row.incoming;
	const kinds = new Set<ProblemKind>();
	// A closest match with anything else against it is no longer a safe one.
	if (note === CLOSEST) kinds.add(warning ? 'match' : 'closest');
	else if (note) kinds.add(kindOf(note));
	if (warning) kinds.add(kindOf(warning));
	return [...kinds];
}

/** The one doubt a person can settle for many rows at once: nothing but the name differs. */
export function canKeepMatch(row: ReviewRow): boolean {
	return row.include && problemsOf(row).join() === 'closest';
}

export interface RowGroup<T> {
	key: string;
	rows: T[];
}

/** Rows of one title together, where its first row was, its seasons in order. */
export function groupRows<T extends Foldable>(rows: T[]): RowGroup<T>[] {
	const groups = new Map<string, RowGroup<T>>();
	for (const row of rows) {
		const key = row.chosen ? titleKey(row.chosen) : row.incoming.key;
		const group = groups.get(key);
		if (group) group.rows.push(row);
		else groups.set(key, { key, rows: [row] });
	}
	const list = [...groups.values()];
	for (const group of list) if (group.rows.length > 1) group.rows.sort(bySeason);
	return list;
}

/** Fewer episodes of a show than this reads as a look at it, not as watching it. */
export const SHOW_THRESHOLD = 3;

/** Under this many minutes of a film, when the file says, is not having seen it. */
const FILM_MINUTES = 15;

export const THRESHOLD_OPTIONS = [
	{ value: '0', label: 'Keep every show' },
	...[2, 3, 4, 5, 6, 8, 10].map((n) => ({
		value: String(n),
		label: `Under ${n} episodes left out`
	}))
];

/**
 * Leave out what was only looked at, on a Netflix import.
 *
 * A show goes by every episode watched across all its rows, so one episode of
 * a later season is not left out when the first was seen. Plays of the same
 * episode count once, because Netflix's rows are built from distinct titles.
 * A show already in the library stays: its watches are worth having. A title
 * with no episode named reads as a film whatever TMDB matched it to, so it
 * stays, and films stay too unless the file has durations and they are short.
 *
 * Rows are changed in place, and nothing is ever dropped. Moving the threshold
 * only undoes what it did itself: a row the person ticked or unticked stays
 * theirs. Run it again whenever a row's title changes.
 */
export function holdBack(rows: ReviewRow[], threshold: number): void {
	const episodes = new Map<string, number>();
	const tracked = new Set<string>();
	for (const row of rows) {
		if (row.chosen?.kind !== 'tv' || !row.incoming.netflix) continue;
		const key = titleKey(row.chosen);
		episodes.set(key, (episodes.get(key) ?? 0) + (row.incoming.progress ?? 0));
		if (row.match?.already_tracked) tracked.add(key);
	}

	const isShort = (row: ReviewRow): boolean => {
		if (!row.chosen || !row.incoming.netflix || threshold < 1) return false;
		const key = titleKey(row.chosen);
		if (row.chosen.kind === 'tv') {
			const watched = episodes.get(key) ?? 0;
			return !tracked.has(key) && watched > 0 && watched < threshold;
		}
		const { seconds } = row.incoming.netflix;
		return seconds !== undefined && seconds < FILM_MINUTES * 60;
	};

	for (const row of rows) {
		const short = isShort(row);
		if (short && !row.short && row.include) {
			row.include = false;
			row.held = true;
		} else if (!short && row.held) {
			row.include = true;
			row.held = false;
		}
		row.short = short;
	}
}

/** Put back what `holdBack` left out, or leave it out again as if nobody had. */
export function setHeld(rows: ReviewRow[], held: boolean): void {
	for (const row of rows) {
		row.include = !held;
		row.held = held;
	}
}

/** "3 seasons · 42 episodes", under a show's name. */
export function groupSummary(rows: Foldable[]): string {
	const seasons = new Set(rows.map((r) => r.incoming.season).filter((s) => s != null));
	const episodes = rows.reduce((n, r) => n + (r.incoming.progress ?? 0), 0);
	const parts = [
		seasons.size === rows.length ? `${seasons.size} seasons` : `${rows.length} entries`
	];
	if (episodes) parts.push(episodes === 1 ? '1 episode' : `${episodes} episodes`);
	return parts.join(' · ');
}
