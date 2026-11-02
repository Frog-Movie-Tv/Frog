import type { ActivityItem } from './types';

/** What someone did, after their name: "watched 6 episodes of Dark". */
export function activityLine(item: ActivityItem): string {
	const name = item.title.name;
	if (item.did === 'finished') return `finished ${name}`;
	if (item.episodes > 1) return `${item.did} ${item.episodes} episodes of ${name}`;
	if (item.episodes === 1 && item.episode !== null) {
		return `${item.did} S${item.season} E${item.episode} of ${name}`;
	}
	return `${item.did} ${name}`;
}

/** The last line of a person's cards, for what did not get a card of its own. */
export const alsoLine = (also: number) =>
	also === 1 ? 'and 1 other title' : `and ${also} other titles`;
