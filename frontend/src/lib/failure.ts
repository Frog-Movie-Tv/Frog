import { ApiError } from './api';

/** What a page says when the thing it shows could not be loaded. */
export interface Failure {
	/** Nothing is there, as opposed to something going wrong on the way. */
	missing: boolean;
	title: string;
	hint: string;
	/** For the tab: "Not found" or "Something went wrong". */
	heading: string;
}

/**
 * Turn a failed load into words for the page. The server's own message is
 * never shown here: it is written for the log, and on a detail page it reads
 * like a stack trace ("TMDB has nothing at /tv/…"). Only the kind of failure
 * decides what the page says; `missing` is the page's own wording for a 404.
 */
export function failure(e: unknown, missing: { title: string; hint: string }): Failure {
	const status = e instanceof ApiError ? e.status : -1;
	if (status === 404 || status === 400) {
		return { missing: true, heading: 'Not found', ...missing };
	}
	if (status === 0) {
		return {
			missing: false,
			heading: 'Offline',
			title: "Can't reach Frog",
			hint: 'Check your connection and try again.'
		};
	}
	if (status === 502) {
		return {
			missing: false,
			heading: 'Something went wrong',
			title: "TMDB isn't answering",
			hint: 'Try again in a minute.'
		};
	}
	if (status === 503) {
		return {
			missing: false,
			heading: 'Unavailable',
			title: "Details aren't available",
			hint: "This Frog isn't connected to TMDB yet."
		};
	}
	return {
		missing: false,
		heading: 'Something went wrong',
		title: 'Something went wrong',
		hint: 'Try again in a minute.'
	};
}

/** A TMDB or database id from the address, or null when it cannot be one. */
export function idParam(raw: string | undefined): number | null {
	return raw && /^\d{1,12}$/.test(raw) && Number(raw) > 0 ? Number(raw) : null;
}
