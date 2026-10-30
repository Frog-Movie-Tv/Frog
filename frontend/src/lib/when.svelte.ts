export type WhenMode = 'today' | 'release' | 'date' | 'unknown';

export interface WhenAnswer {
	when: WhenMode;
	/** yyyy-mm-dd, only for `date`. */
	date?: string;
	/** Which of the offered extras were ticked, by key. */
	extras?: number[];
}

/** Something to tick on top, such as one special of a completed show. */
export interface Extra {
	key: number;
	label: string;
	hint?: string;
}

export interface AskOptions {
	/** Offer "Today". Off when moving something that already has a date. */
	allowToday?: boolean;
	/** A film has one release date, not one per episode. */
	film?: boolean;
	/** Moving an existing entry: start the date picker on its day. */
	day?: string;
	/** The confirm button; "Mark watched" unless this is a move. */
	confirm?: string;
	extras?: Extra[];
	extrasLabel?: string;
}

interface Pending extends Required<Omit<AskOptions, 'day'>> {
	title: string;
	detail: string;
	day: string | null;
	resolve: (answer: WhenAnswer | null) => void;
}

/**
 * Asks "When did you watch this?" and resolves with the answer.
 *
 * One dialog, mounted once in the layout, driven by a promise: callers write
 * `const answer = await whenWatched.ask(...)` and get `null` if it was
 * cancelled, which keeps every bulk action a straight line of code.
 */
class WhenWatched {
	pending = $state<Pending | null>(null);

	ask(title: string, detail: string, options: AskOptions = {}): Promise<WhenAnswer | null> {
		// A second request replaces the first rather than stacking dialogs.
		this.pending?.resolve(null);
		return new Promise((resolve) => {
			this.pending = {
				title,
				detail,
				allowToday: options.allowToday ?? true,
				film: options.film ?? false,
				day: options.day ?? null,
				confirm: options.confirm ?? 'Mark watched',
				extras: options.extras ?? [],
				extrasLabel: options.extrasLabel ?? '',
				resolve
			};
		});
	}

	answer(answer: WhenAnswer | null) {
		this.pending?.resolve(answer);
		this.pending = null;
	}
}

export const whenWatched = new WhenWatched();
