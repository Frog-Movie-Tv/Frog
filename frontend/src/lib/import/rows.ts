import type { MatchResult, SearchItem } from '#lib/types';
import { titleKey } from '#lib/titles';
import type { Incoming } from './common';

/** What folding needs from a row: the tracker's entry and the title chosen for it. */
export interface Foldable {
	incoming: Incoming;
	match: MatchResult | null;
	chosen: SearchItem | null;
}

/** A matched row as the library import takes it. */
export function applyItem(row: Foldable) {
	const chosen = row.chosen!;
	return {
		kind: chosen.kind,
		// TMDB's id, or for an anime TMDB lacks, AniList's. Never Frog's own:
		// the server matches by provider ids.
		tmdb_id: chosen.tmdb_id,
		anilist_id: chosen.anilist_id ?? null,
		status: row.incoming.status,
		rating: row.incoming.rating,
		watched_at: row.incoming.watched_at,
		favourite: row.incoming.favourite,
		rewatch: row.incoming.rewatch,
		season: chosen.kind === 'tv' ? (row.incoming.season ?? null) : null,
		progress: row.incoming.progress ?? null,
		episode_offset: row.incoming.episode_offset ?? 0,
		episode_total: row.incoming.episode_total ?? null,
		episodes: chosen.kind === 'tv' ? (row.incoming.watches ?? []) : []
	};
}

/** Seasons of one show in order, so the latest season is applied last. */
export function bySeason(a: Foldable, b: Foldable): number {
	return (
		titleKey(a.chosen!).localeCompare(titleKey(b.chosen!)) ||
		(a.incoming.season ?? 0) - (b.incoming.season ?? 0) ||
		(a.incoming.episode_offset ?? 0) - (b.incoming.episode_offset ?? 0)
	);
}

/**
 * TMDB often keeps an anime as one long season where trackers count
 * several ("Season 2" is TMDB's season 1 from episode 25). When the earlier
 * seasons are in the same list, their episode counts say where the later one
 * starts. Rows are changed in place.
 */
export function foldSeasons(rows: Foldable[]): void {
	const shows = new Map<string, Foldable[]>();
	for (const row of rows) {
		if (row.chosen?.kind !== 'tv' || row.incoming.season == null) continue;
		const key = titleKey(row.chosen);
		const list = shows.get(key) ?? [];
		list.push(row);
		shows.set(key, list);
	}

	for (const list of shows.values()) {
		const last = list.find((r) => r.match?.last_season != null)?.match?.last_season;
		if (last == null || last < 1) continue;
		list.sort(
			(a, b) =>
				a.incoming.season! - b.incoming.season! ||
				(a.incoming.episode_offset ?? 0) - (b.incoming.episode_offset ?? 0)
		);

		// `carried` is how far into TMDB's last season the rows so far reach;
		// each later tracker season starts where the one before it ended.
		let carried = 0;
		let base = 0;
		let current = -1;
		for (const row of list) {
			const season = row.incoming.season!;
			if (season < last) continue;
			if (season !== current) {
				// A gap means an episode count is missing; stop guessing.
				if (season > last && season !== current + 1) break;
				base = carried;
				current = season;
			}
			if (season > last) {
				const start = base + (row.incoming.episode_offset ?? 0);
				row.incoming.season = last;
				row.incoming.episode_offset = start;
				row.incoming.info = `Counted as season ${last}, from episode ${start + 1}`;
				if (row.match?.note?.startsWith('TMDB lists no season')) row.match.note = null;
			}
			if (row.incoming.episode_total == null) break;
			carried = Math.max(carried, (row.incoming.episode_offset ?? 0) + row.incoming.episode_total);
		}
	}
}
