import type { FrameReport, Signals } from './signals.ts';
import type { VideoSample } from './playback.ts';

export type Confidence = 'exact' | 'likely' | 'guess';

/** A title Frog offers, as `/api/ext/resolve` answers. */
export interface Candidate {
	title_id: number | null;
	tmdb: { kind: string; id: number } | null;
	name: string;
	kind: 'movie' | 'tv';
	year: number | null;
	poster_path: string | null;
	in_library: boolean;
}

export interface Match extends Candidate {
	confidence: Confidence;
	season: number | null;
	episode: number | null;
	episode_name: string | null;
}

export interface Resolution {
	match: Match | null;
	alternatives: Candidate[];
}

export interface NextEpisode {
	season_number: number;
	episode_number: number;
	name: string | null;
	aired: boolean;
}

export interface Me {
	username: string;
	watching: {
		title_id: number;
		name: string;
		poster_path: string | null;
		next: NextEpisode | null;
	}[];
}

export interface Settings {
	/** Frog's address, `https://…`, no trailing slash. */
	url: string;
	token: string;
	/** How far in a video counts as watched, in percent. */
	threshold: number;
	/** Ask before logging even when the match is exact. */
	askFirst: boolean;
	/** Domains where nothing is detected, while the allowlist is off. */
	ignored: string[];
	/** Detect only on `allowed`, instead of everywhere but `ignored`. */
	allowOnly: boolean;
	/** The only domains detected on, while the allowlist is on. */
	allowed: string[];
}

export const DEFAULT_SETTINGS: Settings = {
	url: '',
	token: '',
	threshold: 90,
	askFirst: false,
	ignored: [],
	allowOnly: false,
	allowed: []
};

/** A watch on its way to Frog. Kept until Frog has it, whatever happens in between. */
export interface Pending {
	id: string;
	title_id?: number;
	tmdb_id?: number;
	kind?: string;
	season?: number;
	episode?: number;
	watched_at: string;
	/** What it is, as a person reads it: "Severance S2E7". */
	label: string;
	tries: number;
	/** When to try again, ms since the epoch. */
	next: number;
	/** Set when Frog refused it for good; it waits for you to retry or discard it. */
	failed?: string;
	/** The tab and sitting it came from, so its toast can offer Undo. */
	tab?: number;
}

/** What the popup draws for the tab it was opened over. */
export interface TabView {
	domain: string | null;
	/** Nothing is detected on this tab's site. */
	ignored: boolean;
	/** Sites are detected on only when allowed. */
	allowOnly: boolean;
	/** `seen`: already watched, so Frog left it as it was. */
	state: 'idle' | 'resolving' | 'matched' | 'unknown' | 'logged' | 'queued' | 'seen';
	match: Match | null;
	alternatives: Candidate[];
	/** The page's own name for it, to start a search with. */
	guess: string;
	progress: number | null;
	logId: number | null;
	pendingId: string | null;
	hasRule: boolean;
}

export type FromContent =
	/** `report` is null when it has not changed since the last one. */
	| { type: 'report'; report: FrameReport | null; video: VideoSample | null }
	| { type: 'toast-action'; toast: string; action: string };

export type ToastAction = { label: string; action: string; primary?: boolean };

export type ToContent = {
	type: 'toast';
	id: string;
	text: string;
	actions: ToastAction[];
	/** Seconds before it goes on its own. */
	seconds: number;
};

export type FromPopup =
	| { type: 'view'; tab: number }
	| { type: 'confirm'; tab: number }
	| {
			type: 'choose';
			tab: number;
			candidate: Candidate;
			season: number | null;
			episode: number | null;
	  }
	| { type: 'undo'; tab: number }
	| { type: 'search'; tab: number; query: string }
	| { type: 'site'; tab: number; on: boolean }
	| { type: 'flush' }
	| { type: 'retry'; id: string }
	| { type: 'discard'; id: string }
	| { type: 'me' };

export type { Signals };
