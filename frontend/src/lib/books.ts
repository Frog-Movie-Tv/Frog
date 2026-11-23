/**
 * Shelves drawn as a bookcase: how thick each title's spine is, how many stand
 * face out, the orders a shelf's titles can be put in, and when a
 * watchlist is done. Pure, so `tests/books.test.mjs` runs it without a browser.
 */

/** The thinnest spine, for anything 45 minutes or shorter. */
export const THIN = 13;
/** The thickest, for two hundred hours and more. */
export const THICK = 42;
const SHORT = 45;
const LONG = 12000;

/**
 * A spine's width in pixels from the title's whole length. Logarithmic, so a
 * short series still reads thicker than a long film without One Piece taking
 * the shelf; the curve spreads films apart, where most of a shelf sits. A
 * title with no known length stands at a film's width.
 */
export function spineWidth(minutes: number): number {
	if (!minutes || minutes <= 0) return 20;
	const at = Math.log(Math.max(minutes, SHORT) / SHORT) / Math.log(LONG / SHORT);
	return Math.round(THIN + (THICK - THIN) * Math.min(1, at) ** 0.7);
}

/** How tall a spine stands, as part of a poster's height: books vary. Fixed
 *  per title, so a shelf looks the same every time it is drawn. */
export function spineHeight(id: number): number {
	return 0.8 + ((id * 37) % 19) / 100;
}

export interface Standing<T> {
	/** Face out, first in the shelf's order. */
	faces: T[];
	/** By their spines, after. */
	spines: T[];
	/** What does not fit, lying in a pile. */
	rest: number;
}

/**
 * What stands on a shelf `width` pixels wide: up to three-fifths of it face
 * out (never fewer than three), spines in what is left, and the rest counted.
 * `room` is kept free at the end for the pile and the bookend.
 */
export function stand<T extends { minutes: number }>(
	books: T[],
	total: number,
	width: number,
	face: number,
	gap: number,
	room = 120
): Standing<T> {
	const space = Math.max(0, width - room);
	const fit = Math.max(3, Math.floor((space * 0.6 + gap) / (face + gap)));
	const faces = books.slice(0, Math.min(books.length, fit));
	let free = space - faces.length * (face + gap);
	const spines: T[] = [];
	for (const book of books.slice(faces.length)) {
		const w = spineWidth(book.minutes) + 3;
		if (free < w) break;
		free -= w;
		spines.push(book);
	}
	return { faces, spines, rest: Math.max(0, total - faces.length - spines.length) };
}

export type ShelfFilter = 'all' | 'watchlist' | 'manual' | 'smart';

interface ShelfLike {
	kind: 'manual' | 'smart' | 'watchlist';
}

export function shown(shelf: ShelfLike, filter: ShelfFilter): boolean {
	return filter === 'all' || shelf.kind === filter;
}

export type ItemSort = 'order' | 'year' | 'name' | 'short' | 'long';

interface ItemLike {
	id: number;
	name: string;
	release_date: string | null;
}

interface LengthLike {
	left: number;
	watched: boolean;
}

/**
 * A shelf's titles in the order asked for. Year goes newest first unless
 * `oldest`; undated titles go last either way. Shortest and longest go by
 * what is left to watch, with anything watched at the end.
 */
export function sortItems<T extends ItemLike>(
	items: T[],
	by: ItemSort,
	lengths: Record<string, LengthLike>,
	oldest = false
): T[] {
	const list = [...items];
	const left = (i: T) => lengths[String(i.id)]?.left ?? 0;
	const done = (i: T) => Number(lengths[String(i.id)]?.watched ?? false);
	const year = (i: T) => (i.release_date ? Number(i.release_date.slice(0, 4)) : NaN);
	if (by === 'year')
		list.sort((a, b) => {
			const ya = year(a),
				yb = year(b);
			if (Number.isNaN(ya) || Number.isNaN(yb))
				return Number(Number.isNaN(ya)) - Number(Number.isNaN(yb));
			return oldest ? ya - yb : yb - ya;
		});
	if (by === 'name') list.sort((a, b) => a.name.localeCompare(b.name));
	if (by === 'short') list.sort((a, b) => done(a) - done(b) || left(a) - left(b));
	if (by === 'long') list.sort((a, b) => done(a) - done(b) || left(b) - left(a));
	return list;
}

/** The day a watchlist is done at `hours` a day, starting today. */
export function doneBy(minutesLeft: number, hours: number, today: Date): Date {
	const days = Math.ceil(minutesLeft / (hours * 60));
	const day = new Date(today);
	day.setDate(day.getDate() + days);
	return day;
}
