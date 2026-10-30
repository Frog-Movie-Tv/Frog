import { titleKey } from '#lib/titles';
import { bySeason, type Foldable } from './rows';

/**
 * Sorting an import's rows for review: what is the matter with each one, and
 * which rows belong to the same show.
 */

export interface ReviewRow extends Foldable {
	include: boolean;
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
