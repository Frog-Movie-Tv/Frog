import type { Kind } from '#lib/types';

/** A title as anything in the app may name it: by Frog's id, or by a provider's. */
export interface TitleRef {
	/** Frog's id, once the catalog has the title. */
	id?: number | null;
	kind?: Kind | null;
	tmdb_id?: number | null;
	anilist_id?: number | null;
}

/**
 * Where a title's page is. Its own address, `/t/{id}`, when Frog has it;
 * otherwise the address its provider gives it (`/title/tv/1399`,
 * `/title/anilist/21`), which opens it and moves to its own address.
 */
export function titleHref(t: TitleRef): string | null {
	if (t.id) return `/t/${t.id}`;
	if (t.kind && t.tmdb_id) return `/title/${t.kind}/${t.tmdb_id}`;
	if (t.anilist_id) return `/title/anilist/${t.anilist_id}`;
	return null;
}

/** Whether two references name the same title. */
export function sameTitle(a: TitleRef, b: TitleRef): boolean {
	if (a.id && b.id) return a.id === b.id;
	if (a.tmdb_id && b.tmdb_id) return a.kind === b.kind && a.tmdb_id === b.tmdb_id;
	return !!a.anilist_id && a.anilist_id === b.anilist_id;
}

/** A stable key for a list of references. */
export function titleKey(t: TitleRef): string {
	if (t.id) return `t${t.id}`;
	if (t.tmdb_id) return `${t.kind}${t.tmdb_id}`;
	return `a${t.anilist_id ?? ''}`;
}
