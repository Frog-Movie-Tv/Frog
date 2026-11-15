import { api, frogId } from './api';
import { completeShow } from './complete';
import { menu, showFrom, type MenuEntry, type MenuSource } from './menu.svelte';
import { settings } from './settings.svelte';
import type { TitleRef } from './titles';
import { toasts } from './toast.svelte';
import { statusLabel, type Kind, type Status } from './types';
import { whenWatched } from './when.svelte';

/*
 * Putting a title into the library, wherever its + is: Search, a title's
 * page, the right-click menu. The + asks for a status unless Settings says
 * which one it means (`addStatus`), and asking is the same small menu that a
 * right-click opens, a sheet on a phone, with the likely choice focused so
 * + then Enter is all it takes.
 */

/** A title that can be added: enough to find it and to name it. */
export interface Addable extends TitleRef {
	name: string;
	kind: Kind;
}

/** The order a picker offers statuses in: where most things start, first. */
export const ADD_ORDER: Status[] = ['planned', 'watching', 'completed', 'paused', 'dropped'];

/** The status the + means without asking, or null when it asks. */
export function oneTap(): Status | null {
	const s = settings.current.addStatus;
	return s && s !== 'ask' ? s : null;
}

/** The choice a picker focuses: the one-tap status when there is one, else the Watchlist. */
function likely(): Status {
	return oneTap() ?? 'planned';
}

/** One status picker's rows: the statuses in a title's own words. */
export function statusEntries(
	kind: Kind,
	pick: (s: Status) => void,
	opts: { current?: Status | null; focus?: Status | null; keys?: boolean } = {}
): MenuEntry[] {
	return ADD_ORDER.map((s, i) => ({
		label: statusLabel(s, kind),
		checked: opts.current === undefined ? undefined : s === opts.current,
		focus: s === (opts.focus ?? opts.current),
		key: opts.keys ? String(i + 1) : undefined,
		run: () => pick(s)
	}));
}

/**
 * Ask for a status in a small menu under `from` (a button) or at a point.
 * Settles with the choice, or null when it was closed without one.
 */
export function chooseStatus(
	t: { name: string; kind: Kind },
	from: HTMLElement | { x: number; y: number },
	opts: { event?: MouseEvent | PointerEvent; current?: Status | null; title?: string } = {}
): Promise<Status | null> {
	return new Promise((resolve) => {
		let settled = false;
		const settle = (s: Status | null) => {
			if (settled) return;
			settled = true;
			resolve(s);
		};
		const source: MenuSource = {
			title: opts.title ?? t.name,
			items: () =>
				statusEntries(t.kind, settle, {
					current: opts.current,
					focus: opts.current ?? likely()
				}),
			onclose: () => settle(null)
		};
		if (from instanceof HTMLElement) showFrom(from, source, opts.event);
		else {
			const touch =
				!!opts.event && 'pointerType' in opts.event && opts.event.pointerType === 'touch';
			menu.show(source, from, touch);
		}
	});
}

/** What the + gave: the status, then Frog's id once it is in. */
export interface AddResult {
	id: number;
	status: Status;
}

/**
 * The + itself: asks (or not, per Settings), adds, and says so with Undo.
 * Null when nothing was added: the menu or the "when" question was closed.
 */
export async function addFrom(
	t: Addable,
	from: HTMLElement | { x: number; y: number },
	event?: MouseEvent | PointerEvent,
	changed?: () => unknown
): Promise<AddResult | null> {
	const status = oneTap() ?? (await chooseStatus(t, from, { event }));
	if (!status) return null;
	return addAs(t, status, changed);
}

/**
 * Into the library with this status. Completed means seen, so it asks when,
 * as moving a title there does; a show then ticks every aired episode.
 */
export async function addAs(
	t: Addable,
	status: Status,
	changed?: () => unknown
): Promise<AddResult | null> {
	try {
		const id = await frogId(t);
		if (status === 'completed' && t.kind === 'tv') {
			// A show asks its own question, specials and all, once it is in.
			await api.bulk({ action: 'add', status: 'planned', title_ids: [id] });
			const done = await completeShow(id, t.name);
			if (!done) {
				await api.bulk({ action: 'remove', title_ids: [id] });
				return null;
			}
			await api.track(id, { status: 'completed' });
		} else if (status === 'completed') {
			const reading = t.kind === 'manga';
			const answer = await whenWatched.ask(
				t.name,
				reading ? 'Every chapter' : 'Marked as watched',
				{ film: t.kind === 'movie', reading }
			);
			if (!answer) return null;
			await api.bulk({ action: 'add', status, title_ids: [id], ...answer });
		} else {
			await api.bulk({ action: 'add', status, title_ids: [id] });
		}
		toasts.undoable(`Added ${t.name} to ${statusLabel(status, t.kind)}`, async () => {
			await api.bulk({ action: 'remove', title_ids: [id] });
			await changed?.();
		});
		await changed?.();
		return { id, status };
	} catch (e) {
		toasts.error(e);
		return null;
	}
}

/** A title picked from a provider's list: by Frog's id when it has one, else by TMDB's. */
export interface Pickable {
	id?: number | null;
	kind: Kind;
	tmdb_id?: number | null;
	name: string;
}

/** As many as one request may add; a whole studio goes in a few. */
const AT_ONCE = 100;

/**
 * Many titles into the library with one status, as one action: Completed
 * asks when once, for all of them, and Undo takes every one back out.
 */
export async function addMany(titles: Pickable[], status: Status, changed?: () => unknown) {
	let answer: Awaited<ReturnType<typeof whenWatched.ask>> | undefined;
	if (status === 'completed') {
		const shows = titles.some((t) => t.kind === 'tv');
		answer = await whenWatched.ask(
			titles.length === 1 ? titles[0].name : `${titles.length} titles`,
			shows ? 'Every aired episode' : 'Marked as watched',
			{ film: !shows }
		);
		if (!answer) return null;
	}
	const added: number[] = [];
	let skipped = 0;
	let notOut = 0;
	try {
		for (let i = 0; i < titles.length; i += AT_ONCE) {
			const batch = titles.slice(i, i + AT_ONCE);
			const title_ids = batch.filter((t) => t.id).map((t) => t.id as number);
			const refs = batch
				.filter((t) => !t.id && t.tmdb_id && (t.kind === 'movie' || t.kind === 'tv'))
				.map((t) => ({ kind: t.kind as 'movie' | 'tv', tmdb_id: t.tmdb_id as number }));
			const r = await api.bulk({ action: 'add', status, title_ids, refs, ...(answer ?? {}) });
			added.push(...r.added);
			skipped += r.skipped;
			notOut += r.not_out;
		}
	} catch (e) {
		toasts.error(e);
	}
	if (added.length) {
		const what = added.length === 1 ? titles[0].name : `${added.length} titles`;
		const notes = [
			skipped ? `${skipped} ${skipped === 1 ? 'was' : 'were'} already in your library` : '',
			notOut ? `${notOut} not out yet` : ''
		].filter(Boolean);
		toasts.undoable(
			`Added ${what} to ${statusLabel(status)}${notes.length ? `. ${notes.join(', ')}.` : ''}`,
			async () => {
				for (let i = 0; i < added.length; i += AT_ONCE) {
					await api.bulk({ action: 'remove', title_ids: added.slice(i, i + AT_ONCE) });
				}
				await changed?.();
			}
		);
	} else if (skipped) {
		toasts.push('Already in your library');
	}
	await changed?.();
	return { added, skipped };
}
