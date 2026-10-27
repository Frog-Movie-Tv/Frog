import { api } from '#lib/api';
import { settings } from '#lib/settings.svelte';
import { toasts } from '#lib/toast.svelte';
import type { ImportSummary } from '#lib/types';
import { dedupe, readAniList } from './parsers';
import { applyItem, bySeason, foldSeasons, type Foldable } from './rows';

/** How long a sync stays fresh before opening the app runs another. */
const EVERY_MS = 6 * 60 * 60 * 1000;

export interface SyncOutcome extends ImportSummary {
	/** Titles that need a choice on the Import page; sync leaves them alone. */
	unsure: number;
}

/**
 * Keeps the library in step with a public AniList list. It only ever adds
 * and moves forward (the server's merge mode), so it is safe to run unasked.
 */
class AniListSync {
	running = $state(false);
	last = $state<SyncOutcome | null>(null);

	/** On app open: quietly, and only when the last sync has gone stale. */
	async maybeRun() {
		const s = settings.current;
		if (!s.anilistUser || this.running) return;
		const at = s.anilistSyncedAt ? Date.parse(s.anilistSyncedAt) : 0;
		if (Date.now() - at < EVERY_MS) return;
		try {
			const outcome = await this.run(s.anilistUser);
			const changed = outcome.added + outcome.updated;
			if (changed) toasts.push(describe(outcome));
		} catch {
			// A background sync that fails stays quiet; Settings shows it on demand.
		}
	}

	async run(username: string): Promise<SyncOutcome> {
		this.running = true;
		try {
			const outcome = await sync(username);
			this.last = outcome;
			settings.patch({ anilistUser: username.trim(), anilistSyncedAt: new Date().toISOString() });
			return outcome;
		} finally {
			this.running = false;
		}
	}
}

async function sync(username: string): Promise<SyncOutcome> {
	const parsed = (await readAniList(username)).flatMap((chunk) => chunk.rows);
	const rows: Foldable[] = dedupe(parsed).map((incoming) => ({
		incoming,
		match: null,
		chosen: null
	}));

	for (let i = 0; i < rows.length; i += 40) {
		const batch = rows.slice(i, i + 40);
		const { results } = await api.matchImport(batch.map((r) => r.incoming));
		for (const result of results) {
			const row = batch.find((r) => r.incoming.key === result.key);
			if (!row) continue;
			row.match = result;
			row.chosen = result.matched;
		}
	}
	foldSeasons(rows);

	// Only what the Import page would apply without asking anything.
	const sure = rows.filter((r) => r.chosen && !r.match?.note && !r.incoming.warning).sort(bySeason);

	// AniList lists each season on its own; Frog keeps one status per show,
	// which is the latest season's. Merging only moves forward, so an earlier
	// "completed" season would otherwise hold a show you are still watching.
	for (let i = sure.length - 1; i > 0; i--) {
		const later = sure[i].incoming;
		const earlier = sure[i - 1].incoming;
		const sameShow =
			sure[i - 1].chosen!.kind === 'tv' && sure[i - 1].chosen!.tmdb_id === sure[i].chosen!.tmdb_id;
		if (!sameShow || earlier.status === later.status) continue;
		// A finished season is ticked because it is "completed"; once it takes
		// the show's status it needs its episode count instead, so one without
		// a count keeps its own status rather than losing its episodes.
		if (earlier.status === 'completed') {
			if (earlier.episode_total == null) continue;
			earlier.progress = earlier.episode_total;
		}
		earlier.status = later.status;
	}

	const outcome: SyncOutcome = {
		added: 0,
		updated: 0,
		skipped: 0,
		failed: [],
		unsure: rows.length - sure.length
	};
	for (let i = 0; i < sure.length; i += 25) {
		const result = await api.applyImport(sure.slice(i, i + 25).map(applyItem), 'merge');
		outcome.added += result.added;
		outcome.updated += result.updated;
		outcome.skipped += result.skipped;
		outcome.failed.push(...result.failed);
	}
	return outcome;
}

/** "2 added and 1 updated from AniList", for a toast or a status line. */
export function describe(o: SyncOutcome): string {
	const parts = [];
	if (o.added) parts.push(`${o.added} added`);
	if (o.updated) parts.push(`${o.updated} updated`);
	if (!parts.length) return 'In step with AniList';
	return `${parts.join(' and ')} from AniList`;
}

export const anilist = new AniListSync();
