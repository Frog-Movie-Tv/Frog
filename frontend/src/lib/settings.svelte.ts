import { browser } from '$app/env';
import type { ScoreFormat } from './format';

export type ThemeName = 'dark' | 'amoled' | 'light';

export interface Settings {
	theme: ThemeName;
	scoreFormat: ScoreFormat;
	/** Poster column width in px; the grid is auto-fill, so this is density. */
	posterSize: number;
	/** Tint pages with the colour pulled from the poster. */
	followPoster: boolean;
	/** Hide overviews and ratings for things not yet watched. */
	hideSpoilers: boolean;
}

const DEFAULTS: Settings = {
	theme: 'dark',
	scoreFormat: 'pt10dec',
	posterSize: 132,
	followPoster: true,
	hideSpoilers: false
};

const KEY = 'frog.settings.v1';

function load(): Settings {
	if (!browser) return { ...DEFAULTS };
	try {
		const raw = localStorage.getItem(KEY);
		if (!raw) return { ...DEFAULTS };
		return { ...DEFAULTS, ...(JSON.parse(raw) as Partial<Settings>) };
	} catch {
		return { ...DEFAULTS };
	}
}

/**
 * Preferences live in the browser for now. Phase 3 moves them into
 * `app_setting` so a new device inherits them, which is also what makes
 * shareable theme JSON possible.
 */
class SettingsStore {
	current = $state<Settings>(load());

	set<K extends keyof Settings>(key: K, value: Settings[K]) {
		this.current = { ...this.current, [key]: value };
		this.persist();
	}

	reset() {
		this.current = { ...DEFAULTS };
		this.persist();
	}

	private persist() {
		if (!browser) return;
		try {
			localStorage.setItem(KEY, JSON.stringify(this.current));
		} catch {
			// Private mode or a full quota: preferences are not worth an error.
		}
		this.apply();
	}

	apply() {
		if (!browser) return;
		const root = document.documentElement;
		root.dataset.theme = this.current.theme;
		root.style.setProperty('--poster-min', `${this.current.posterSize}px`);
	}
}

export const settings = new SettingsStore();
