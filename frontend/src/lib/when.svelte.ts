export type WhenMode = 'today' | 'release' | 'date' | 'unknown';

export interface WhenAnswer {
	when: WhenMode;
	/** yyyy-mm-dd, only for `date`. */
	date?: string;
}

interface Pending {
	title: string;
	detail: string;
	allowToday: boolean;
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

	ask(title: string, detail: string, allowToday = true): Promise<WhenAnswer | null> {
		// A second request replaces the first rather than stacking dialogs.
		this.pending?.resolve(null);
		return new Promise((resolve) => {
			this.pending = { title, detail, allowToday, resolve };
		});
	}

	answer(answer: WhenAnswer | null) {
		this.pending?.resolve(answer);
		this.pending = null;
	}
}

export const whenWatched = new WhenWatched();
