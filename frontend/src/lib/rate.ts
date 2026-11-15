import type { ScoreFormat } from './format';
import type { LibraryItem } from './types';

/**
 * Rating a library one card at a time (`routes/rate`). What goes in the deck
 * and what the buttons offer are decided here, on plain values, so they can be
 * tested without a page.
 */

/**
 * Titles you finished, or everything in the library including the watchlist.
 * A show still being watched is not finished: it is rated once it is done.
 */
export type DeckScope = 'finished' | 'all';

/**
 * The titles still waiting for a rating. What you watched most recently comes
 * first, while it is fresh; titles with no watch follow, newest added first.
 */
export function deckOf(items: LibraryItem[], scope: DeckScope): LibraryItem[] {
	return items
		.filter(
			(i) =>
				i.entry && i.entry.rating === null && (scope === 'all' || i.entry.status === 'completed')
		)
		.sort(
			(a, b) =>
				(b.last_watched_at ?? '').localeCompare(a.last_watched_at ?? '') ||
				(b.entry?.added_at ?? '').localeCompare(a.entry?.added_at ?? '')
		);
}

/**
 * The scores a card offers, in the format's own units, never more than ten so
 * each is one big button and one key. The fine-grained formats step in tens
 * and whole points; a card is a first impression, and a title page still takes
 * any value.
 */
export function choices(format: ScoreFormat, halfStars: boolean): number[] {
	const run = (from: number, to: number, step: number) => {
		const out: number[] = [];
		for (let v = from; v <= to + 1e-9; v += step) out.push(Math.round(v * 10) / 10);
		return out;
	};
	switch (format) {
		case 'pt100':
			return run(10, 100, 10);
		case 'pt10dec':
		case 'pt10':
			return run(1, 10, 1);
		case 'star5':
			return halfStars ? run(0.5, 5, 0.5) : run(1, 5, 1);
		case 'smiley3':
			return run(1, 3, 1);
	}
}

/** The choice a number key picks: 1 to 9, then 0 for the tenth. */
export function choiceForKey(key: string, count: number): number | null {
	if (!/^[0-9]$/.test(key)) return null;
	const index = key === '0' ? 9 : Number(key) - 1;
	return index < count ? index : null;
}

/**
 * One thing done to a card, in the order done, so Undo takes them back
 * newest first: a score, a skip, a status changed, or the title removed.
 * `left` says whether it took the card out of the pile: a status change on
 * a card in hand leaves it there.
 */
export interface Step<T extends { id: number }> {
	key: number;
	kind: 'rate' | 'skip' | 'move' | 'remove';
	item: T;
	left: boolean;
}

/** How many cards were scored, of the steps still standing. */
export function ratedIn<T extends { id: number }>(steps: Step<T>[]): number {
	return steps.filter((s) => s.kind === 'rate').length;
}

/**
 * Take one step back, wherever it is in the history: Undo takes the newest,
 * and a save the server refused takes back its own. A card that left the
 * pile comes back on top of it, once.
 */
export function takeBack<T extends { id: number }, S extends Step<T>>(
	queue: T[],
	steps: S[],
	key: number
): { queue: T[]; steps: S[]; step: S | null } {
	const step = steps.find((s) => s.key === key) ?? null;
	if (!step) return { queue, steps, step };
	const rest = steps.filter((s) => s.key !== key);
	if (!step.left) return { queue, steps: rest, step };
	return {
		queue: [step.item, ...queue.filter((i) => i.id !== step.item.id)],
		steps: rest,
		step
	};
}
