import { api } from './api';
import { localDay } from './day';
import { prettyDate } from './format';
import { toasts } from './toast.svelte';
import type { LibraryItem } from './types';
import { whenWatched } from './when.svelte';

/**
 * A show marked Completed is a show watched: ask when, as ticking it by hand
 * would, and tick every aired episode of every season. Specials are a choice,
 * so they are offered rather than assumed. The same wherever a show becomes
 * Completed: its page, or added from Search with Completed as the default.
 * Answers the show as it is afterwards, or null when nothing was ticked.
 */
export async function completeShow(titleId: number, name: string): Promise<LibraryItem | null> {
	const specials = await api
		.season(titleId, 0)
		.then((s) =>
			s.episodes
				.filter((e) => !e.watched && (!e.air_date || e.air_date <= localDay()))
				.map((e) => ({
					key: e.episode_number,
					label: e.name || `Special ${e.episode_number}`,
					hint: e.air_date ? prettyDate(e.air_date) : undefined
				}))
		)
		.catch(() => []);
	const answer = await whenWatched.ask(name, 'Every aired episode of every season', {
		extras: specials,
		extrasLabel: 'Specials you saw too'
	});
	if (!answer) return null;
	try {
		const { item } = await api.setShowWatched(titleId, true, answer);
		toasts.push(`Marked ${item.watched_episodes} episodes watched`);
		return item;
	} catch (e) {
		toasts.error(e);
		return null;
	}
}
