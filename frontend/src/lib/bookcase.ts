/**
 * The last answers about shelves, kept in the module so both pages of the
 * zoom draw at once: the Shelves page when Back returns to it, a shelf when
 * it is opened (it is read as the pointer comes near). Without them the
 * browser would picture a spinner at one end and nothing would move. A fresh
 * reading replaces each one straight after.
 *
 * Kept per account, and forgotten when someone else signs in on this browser.
 */
import { api } from './api';
import { session } from './session.svelte';
import type { Length, LibraryItem, Shelf } from './types';

export interface ShelfAnswer {
	shelf: Shelf;
	items: LibraryItem[];
	lengths: Record<string, Length>;
}

let owner: string | null = null;
let list: Shelf[] | null = null;
const opened = new Map<number, ShelfAnswer>();
const asking = new Map<number, Promise<ShelfAnswer>>();

function mine() {
	const who = session.status?.username ?? null;
	if (who === owner) return;
	owner = who;
	list = null;
	opened.clear();
	asking.clear();
}

export function lastShelves(): Shelf[] | null {
	mine();
	return list;
}

export function keepShelves(shelves: Shelf[]) {
	mine();
	list = shelves;
}

export function lastShelf(id: number): ShelfAnswer | null {
	mine();
	return opened.get(id) ?? null;
}

/** Read a shelf, once at a time however often it is asked for. */
export function readShelf(id: number): Promise<ShelfAnswer> {
	mine();
	const already = asking.get(id);
	if (already) return already;
	const who = owner;
	const reading = api
		.shelf(id)
		.then((answer) => {
			if (who === owner) opened.set(id, answer);
			return answer;
		})
		.finally(() => asking.delete(id));
	asking.set(id, reading);
	return reading;
}

/** Start reading a shelf that is about to be opened; failures wait for the page. */
export function readSoon(id: number) {
	if (!lastShelf(id)) readShelf(id).catch(() => {});
}
