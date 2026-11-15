import { api } from './api';

/*
 * The bell's number: how many notifications this account has not seen. Read
 * when the app opens, whenever the window comes back into focus, and every
 * few minutes while it is open. No socket: a Frog is a handful of people,
 * and a few minutes late is soon enough for anything it says.
 */

/** How often an open window asks again. */
const EVERY_MS = 3 * 60 * 1000;

class Inbox {
	unread = $state(0);
	private timer: ReturnType<typeof setInterval> | null = null;

	async refresh() {
		try {
			this.unread = (await api.unread()).unread;
		} catch {
			// Offline or signed out: the number stays as it was.
		}
	}

	/** Start keeping the number fresh; returns how to stop. */
	watch(): () => void {
		void this.refresh();
		const onFocus = () => void this.refresh();
		window.addEventListener('focus', onFocus);
		this.timer = setInterval(() => {
			if (document.visibilityState === 'visible') void this.refresh();
		}, EVERY_MS);
		return () => {
			window.removeEventListener('focus', onFocus);
			if (this.timer) clearInterval(this.timer);
			this.timer = null;
		};
	}

	/** The page said how many are left. */
	set(n: number) {
		this.unread = n;
	}
}

export const inbox = new Inbox();
