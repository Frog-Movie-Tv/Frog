import type { ScoreFormat } from './format';
import type { LibraryItem } from './types';

/**
 * Rating a library one card at a time (`routes/rate`). What goes in the deck
 * and what the buttons offer are decided here, on plain values, so they can be
 * tested without a page.
 */

/** Titles you have seen, or everything in the library including the watchlist. */
export type DeckScope = 'seen' | 'all';

/**
 * The titles still waiting for a rating. What you watched most recently comes
 * first, while it is fresh; titles with no watch follow, newest added first.
 */
export function deckOf(items: LibraryItem[], scope: DeckScope): LibraryItem[] {
	return items
		.filter(
			(i) => i.entry && i.entry.rating === null && (scope === 'all' || i.entry.status !== 'planned')
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
