import { goto } from '$app/navigation';
import { api, frogId } from './api';
import { completeShow } from './complete';
import { episodeCode } from './format';
import type { MenuEntry, MenuSource } from './menu.svelte';
import { settings } from './settings.svelte';
import { titleHref, type TitleRef } from './titles';
import { toasts } from './toast.svelte';
import {
	STATUSES,
	statusLabel,
	type Kind,
	type NextEpisode,
	type Shelf,
	type Status
} from './types';
import { whenWatched } from './when.svelte';

/*
 * What a right-click offers, on a title and on a shelf.
 *
 * Built here once so a title offers the same things wherever its card is:
 * Library, a shelf, Home's rows and Search. Each page says only where the
 * card is (`TitleMenuContext`) and what to read again afterwards. Everything
 * destructive goes through a toast that can take it back, as it does
 * everywhere else in Frog.
 */

/** A title as its card knows it: enough to say what can be done with it. */
export interface MenuTitle extends TitleRef {
	name: string;
	kind: Kind;
	/** Where it stands in your library; null when it is not there. */
	status: Status | null;
	favourite?: boolean;
}

export interface TitleMenuContext {
	/** On a shelf's own page: the shelf the title is on, and how that page takes it off. */
	shelf?: Pick<Shelf, 'id' | 'name' | 'kind'> & { remove?: () => void | Promise<void> };
	/** A card that is mid-way: what comes next, and how far it has got. */
	progress?: { next: NextEpisode | null; watched: number };
	/** Something changed: whatever shows the title reads it again. */
	onchanged?: () => void | Promise<void>;
}

export function titleMenu(title: MenuTitle, ctx: TitleMenuContext = {}): MenuSource {
	return { title: title.name, items: () => titleEntries(title, ctx) };
}

function titleEntries(t: MenuTitle, ctx: TitleMenuContext): MenuEntry[] {
	const reading = t.kind === 'manga';
	const changed = () => ctx.onchanged?.();
	const href = titleHref(t);
	const held = t.status !== null;
	const entries: MenuEntry[] = [];

	if (href) entries.push({ label: 'Open', icon: 'external', run: () => goto(href) });

	const next = nextEntry(t, ctx.progress, changed);
	if (next) entries.push(next);

	if (!held) {
		entries.push({
			label: 'Add to library',
			icon: 'plus',
			run: () => add(t, settings.current.defaultStatus, changed)
		});
		entries.push({
			label: reading ? 'Start reading' : 'Start watching',
			icon: 'play',
			run: () => add(t, 'watching', changed)
		});
	} else {
		entries.push({
			label: 'Move to',
			icon: 'move',
			children: () =>
				STATUSES.map((s) => ({
					label: statusLabel(s, t.kind),
					checked: s === t.status,
					run: () => setStatus(t, s, changed)
				}))
		});
		entries.push({
			label: t.favourite ? 'Remove from favourites' : 'Favourite',
			icon: 'heart',
			run: () => setFavourite(t, !t.favourite, changed)
		});
	}

	entries.push({ label: 'Add to shelf', icon: 'layers', children: () => shelfChoices(t, changed) });

	if ((ctx.shelf && ctx.shelf.kind !== 'smart') || held) entries.push('rule');
	if (ctx.shelf && ctx.shelf.kind !== 'smart') {
		const shelf = ctx.shelf;
		entries.push({
			label: `Remove from ${shelf.name}`,
			icon: 'x',
			danger: true,
			run: () => (shelf.remove ? shelf.remove() : removeFromShelf(t, shelf, changed))
		});
	}
	if (held) {
		entries.push({
			label: 'Remove from library',
			icon: 'trash',
			danger: true,
			run: () => removeFromLibrary(t, changed)
		});
	}

	if (href) {
		entries.push('rule');
		entries.push({ label: 'Copy link', icon: 'link', run: () => copyLink(href) });
	}
	return entries;
}

/** The next episode watched, or the next chapter read, from the card. */
function nextEntry(
	t: MenuTitle,
	progress: TitleMenuContext['progress'],
	changed: () => unknown
): MenuEntry | null {
	if (!progress || !t.id) return null;
	const id = t.id;
	if (t.kind === 'manga') {
		const chapter = progress.watched + 1;
		return {
			label: `Mark chapter ${chapter} read`,
			icon: 'check',
			run: async () => {
				try {
					await api.track(id, { progress: chapter });
					toasts.undoable(`Marked chapter ${chapter} read`, async () => {
						await api.track(id, { progress: chapter - 1 });
						await changed();
					});
					await changed();
				} catch (e) {
					toasts.error(e);
				}
			}
		};
	}
	const next = progress.next;
	if (t.kind !== 'tv' || !next || !next.aired) return null;
	const code = episodeCode(next.season_number, next.episode_number);
	return {
		label: `Mark ${code} watched`,
		icon: 'check',
		run: async () => {
			try {
				await api.setEpisodeWatched(id, next.season_number, next.episode_number, true);
				toasts.undoable(`Marked ${code} watched`, async () => {
					await api.setEpisodeWatched(id, next.season_number, next.episode_number, false);
					await changed();
				});
				await changed();
			} catch (e) {
				toasts.error(e);
			}
		}
	};
}

/** Into your library, as Search's quick buttons do: Completed means seen. */
async function add(t: MenuTitle, status: Status, changed: () => unknown) {
	try {
		const id = await frogId(t);
		const { item } = await api.track(id, { status });
		toasts.push(`Added ${t.name} to ${statusLabel(status, t.kind)}`);
		await changed();
		if (status === 'completed' && item.kind === 'tv' && item.total_episodes > 0) {
			await completeShow(id, item.name);
			await changed();
		}
	} catch (e) {
		toasts.error(e);
	}
}

/** The same request Library's edit bar sends for one title, undo included. */
async function setStatus(t: MenuTitle, status: Status, changed: () => unknown) {
	if (!t.id || status === t.status) return;
	const ids = [t.id];
	try {
		if (status === 'completed') {
			const reading = t.kind === 'manga';
			const answer = await whenWatched.ask(
				t.name,
				reading ? 'Every chapter' : t.kind === 'tv' ? 'Every aired episode' : 'Marked as watched',
				{ film: t.kind === 'movie', reading }
			);
			if (!answer) return;
			const { marked, skipped, before } = await api.bulk({
				action: 'status',
				status,
				title_ids: ids,
				...answer
			});
			const said = skipped
				? `Marked ${t.name} watched. ${skipped} not out yet.`
				: marked
					? `Marked ${t.name} watched`
					: `Moved ${t.name} to ${statusLabel(status, t.kind)}`;
			toasts.undoable(said, async () => {
				await api.bulk({ action: 'unmark', entries: before });
				await changed();
			});
		} else {
			const { before } = await api.bulk({ action: 'status', status, title_ids: ids });
			toasts.undoable(`Moved ${t.name} to ${statusLabel(status, t.kind)}`, async () => {
				await api.bulk({ action: 'restore', entries: before });
				await changed();
			});
		}
		await changed();
	} catch (e) {
		toasts.error(e);
	}
}

async function setFavourite(t: MenuTitle, favourite: boolean, changed: () => unknown) {
	if (!t.id) return;
	try {
		const { before } = await api.bulk({ action: 'favourite', favourite, title_ids: [t.id] });
		toasts.undoable(
			favourite ? `Added ${t.name} to favourites` : `Took ${t.name} off favourites`,
			async () => {
				await api.bulk({ action: 'restore', entries: before });
				await changed();
			}
		);
		await changed();
	} catch (e) {
		toasts.error(e);
	}
}

/** Every shelf, ticked where the title already is; a tap puts it on or takes it off. */
async function shelfChoices(t: MenuTitle, changed: () => unknown): Promise<MenuEntry[]> {
	const id = await frogId(t);
	const { shelves } = await api.titleShelves(id);
	if (!shelves.length) return [{ label: 'No shelves yet', disabled: true }];
	return shelves.map((s) => ({
		label: s.name,
		checked: s.on,
		run: async () => {
			try {
				if (s.on) await api.removeFromShelf(s.id, id);
				else await api.addToShelf(s.id, id);
				toasts.push(s.on ? `Removed from ${s.name}` : `Added to ${s.name}`);
				await changed();
			} catch (e) {
				toasts.error(e);
			}
		}
	}));
}

async function removeFromShelf(
	t: MenuTitle,
	shelf: Pick<Shelf, 'id' | 'name'>,
	changed: () => unknown
) {
	if (!t.id) return;
	const id = t.id;
	try {
		await api.removeFromShelf(shelf.id, id);
		toasts.undoable(`Removed ${t.name}`, async () => {
			await api.addToShelf(shelf.id, id);
			await changed();
		});
		await changed();
	} catch (e) {
		toasts.error(e);
	}
}

async function removeFromLibrary(t: MenuTitle, changed: () => unknown) {
	if (!t.id) return;
	try {
		const { before } = await api.bulk({ action: 'remove', title_ids: [t.id] });
		toasts.undoable(`Removed ${t.name}`, async () => {
			await api.bulk({ action: 'restore', entries: before });
			await changed();
		});
		await changed();
	} catch (e) {
		toasts.error(e);
	}
}

async function copyLink(href: string) {
	try {
		await navigator.clipboard.writeText(new URL(href, location.origin).toString());
		toasts.push('Link copied');
	} catch {
		toasts.push('Could not copy the link', 'error');
	}
}

/** What a shelf's card offers, on Shelves and on Home. */
export function shelfMenu(
	shelf: Pick<Shelf, 'id' | 'name' | 'kind'>,
	onchanged?: () => void | Promise<void>
): MenuSource {
	const href = `/shelves/${shelf.id}`;
	return {
		title: shelf.name,
		items: () => [
			{ label: 'Open', icon: 'external', run: () => goto(href) },
			shelf.kind === 'smart'
				? { label: 'Edit rules', icon: 'filter', run: () => goto(`${href}?edit=1`) }
				: { label: 'Add titles', icon: 'plus', run: () => goto(`${href}?add=1`) },
			{ label: 'Rename', icon: 'pencil', run: () => goto(`${href}?rename=1`) },
			'rule',
			{
				label: 'Delete shelf',
				icon: 'trash',
				danger: true,
				run: async () => {
					try {
						const gone = await api.deleteShelf(shelf.id);
						toasts.undoable(`Deleted ${shelf.name}`, async () => {
							await api.createShelf(gone);
							await onchanged?.();
						});
						await onchanged?.();
					} catch (e) {
						toasts.error(e);
					}
				}
			},
			'rule',
			{ label: 'Copy link', icon: 'link', run: () => copyLink(href) }
		]
	};
}
