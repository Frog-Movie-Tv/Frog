import { api } from './api';
import type { Choosable } from './types';

/*
 * Choosing other people on this Frog: whom to recommend a title to, or whom
 * you watched something with. One dialog, mounted once in the layout
 * (`PeoplePicker.svelte`), driven by a promise like `whenWatched`, so a caller
 * writes `const chosen = await peoplePicker.ask(...)` and gets null when it
 * was closed. The list itself (`PeopleList.svelte`) also sits inside the
 * "when did you watch it" dialog.
 */

export type Purpose = 'recommend' | 'together';

export interface PickOptions {
	/** The dialog's heading: "Recommend Severance". */
	heading: string;
	purpose: Purpose;
	/** Marks who already has this title from you, for a recommendation. */
	titleId?: number;
	/** Ticked as it opens: who a show is being watched with now. */
	picked?: number[];
	/** Offer a short note, collapsed until asked for. */
	note?: boolean;
	/** The confirm button. */
	confirm: string;
	/** Confirming with nobody ticked is an answer too: clearing who you watched with. */
	allowNone?: boolean;
}

export interface Picked {
	ids: number[];
	note: string | null;
}

interface Pending extends PickOptions {
	resolve: (answer: Picked | null) => void;
}

class PeoplePicker {
	pending = $state<Pending | null>(null);

	ask(options: PickOptions): Promise<Picked | null> {
		this.pending?.resolve(null);
		return new Promise((resolve) => {
			this.pending = { ...options, resolve };
		});
	}

	answer(answer: Picked | null) {
		this.pending?.resolve(answer);
		this.pending = null;
	}
}

export const peoplePicker = new PeoplePicker();

/**
 * Whether anyone at all can be chosen, asked once a session per purpose: a
 * Recommend button or a "Watched with" row with nobody behind it is not drawn.
 */
class Reachable {
	recommend = $state<boolean | null>(null);
	together = $state<boolean | null>(null);
	private asked = new Set<Purpose>();

	check(purpose: Purpose) {
		if (this.asked.has(purpose)) return;
		this.asked.add(purpose);
		api
			.choosable(purpose)
			.then((r) => (this[purpose] = r.people.length > 0))
			.catch(() => {
				this.asked.delete(purpose);
				this[purpose] = false;
			});
	}

	/** Someone switched their wishes, or a new account joined: ask again next time. */
	forget() {
		this.asked.clear();
		this.recommend = null;
		this.together = null;
	}
}

export const reachable = new Reachable();

/** A person's name as the app shows it. */
export function personName(p: { username: string; display_name: string | null }): string {
	return p.display_name?.trim() || p.username;
}

/** Where a person's picture is, when they have one. */
export function avatarOf(p: { username: string; avatar: boolean }): string | null {
	return p.avatar ? `/api/profiles/${encodeURIComponent(p.username)}/images/avatar` : null;
}

/** "Sam", "Sam and Alex", "Sam, Alex and 2 more". */
export function namesOf(people: { username: string; display_name: string | null }[]): string {
	const names = people.map(personName);
	if (names.length <= 2) return names.join(' and ');
	if (names.length === 3) return `${names[0]}, ${names[1]} and ${names[2]}`;
	return `${names[0]}, ${names[1]} and ${names.length - 2} more`;
}

/** Filter a list as typed, by name or username, ignoring case. */
export function matching(people: Choosable[], typed: string): Choosable[] {
	const q = typed.trim().toLowerCase();
	if (!q) return people;
	return people.filter(
		(p) => p.username.toLowerCase().includes(q) || personName(p).toLowerCase().includes(q)
	);
}
