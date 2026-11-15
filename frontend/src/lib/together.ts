import { api } from './api';
import { namesOf, peoplePicker } from './people.svelte';
import { toasts } from './toast.svelte';
import type { Companion } from './types';

/*
 * Who you watched something with, naming other accounts: a show's "watching
 * with" (every episode ticked from then on names them), and a diary day's.
 * The people named are told, and choose for themselves.
 */

/** Choose who a show is being watched with; nobody clears it. Answers the new list. */
export async function watchShowWith(
	title: { id: number; name: string },
	current: Companion[]
): Promise<Companion[] | null> {
	const chosen = await peoplePicker.ask({
		heading: `Watching ${title.name} with`,
		purpose: 'together',
		picked: current.map((c) => c.id),
		confirm: 'Save',
		allowNone: true
	});
	if (!chosen) return null;
	try {
		const { with: now } = await api.setTogether(title.id, chosen.ids);
		toasts.push(
			now.length ? `Watching ${title.name} with ${namesOf(now)}` : `Watching ${title.name} alone`
		);
		return now;
	} catch (e) {
		toasts.error(e);
		return null;
	}
}

/** "With Sam", "With Sam and Alex". */
export function withLine(people: Companion[]): string {
	return people.length ? `With ${namesOf(people)}` : '';
}
