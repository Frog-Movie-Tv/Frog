import { browser } from '$app/env';
import type { ScoreFormat } from './format';
import type { Status } from './types';

export type ThemeName = 'pond' | 'pond-light' | 'dark' | 'amoled' | 'light';
export type MascotName = 'classic' | 'sleepy' | 'popcorn' | 'cinema';
export type CardStyle = 'poster' | 'landscape' | 'list' | 'compact';
export type HeroStyle = 'backdrop' | 'poster' | 'off';
export type FontName = 'system' | 'humanist' | 'grotesque' | 'rounded' | 'serif' | 'mono';
export type AnimationLevel = 'full' | 'reduced' | 'off';

export const THEMES: { id: ThemeName; label: string; hint: string; swatches: string[] }[] = [
	{ id: 'pond', label: 'Pond', hint: 'The default', swatches: ['#07120e', '#16302a', '#6cc46b'] },
	{ id: 'pond-light', label: 'Pond Light', hint: 'Cream and green', swatches: ['#f1eee1', '#fbf9f1', '#2f7d4f'] },
	{ id: 'dark', label: 'Dark', hint: 'Deep charcoal', swatches: ['#08080c', '#191922', '#7a5cff'] },
	{ id: 'amoled', label: 'AMOLED', hint: 'True black', swatches: ['#000000', '#121217', '#7a5cff'] },
	{ id: 'light', label: 'Light', hint: 'Paper', swatches: ['#f6f5f8', '#ffffff', '#5b3fd6'] }
];

export const MASCOTS: { id: MascotName; label: string; hint: string }[] = [
	{ id: 'classic', label: 'Classic', hint: 'Wide awake' },
	{ id: 'sleepy', label: 'Sleepy', hint: 'Just one more episode' },
	{ id: 'popcorn', label: 'Popcorn', hint: 'Settled in for a double bill' },
	{ id: 'cinema', label: '3D glasses', hint: 'Front row, centre' }
];

/**
 * Only fonts that are already on the device. Loading a web font would mean a
 * third-party request on every page and a gap in offline mode, which is not
 * worth it for a choice of letterforms.
 */
export const FONTS: { id: FontName; label: string; stack: string }[] = [
	{
		id: 'system',
		label: 'System',
		stack:
			"'Segoe UI Variable Display', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif"
	},
	{ id: 'humanist', label: 'Humanist', stack: "'Segoe UI', Roboto, 'Noto Sans', sans-serif" },
	{ id: 'grotesque', label: 'Grotesque', stack: "'Helvetica Neue', Helvetica, Arial, sans-serif" },
	{
		id: 'rounded',
		label: 'Rounded',
		stack: "'Segoe UI Rounded', 'SF Pro Rounded', 'Nunito', system-ui, sans-serif"
	},
	{ id: 'serif', label: 'Serif', stack: "Georgia, 'Times New Roman', 'Noto Serif', serif" },
	{ id: 'mono', label: 'Mono', stack: "ui-monospace, 'Cascadia Mono', Consolas, monospace" }
];

/** A theme the user built, stored as plain token overrides. */
export interface CustomTheme {
	name: string;
	base: ThemeName;
	tokens: Partial<Record<'bg' | 'bg-elev' | 'bg-elev-2' | 'accent' | 'text', string>>;
}

export interface Settings {
	theme: ThemeName;
	mascot: MascotName;
	customTheme: CustomTheme | null;
	savedThemes: CustomTheme[];

	scoreFormat: ScoreFormat;
	halfStars: boolean;

	cardStyle: CardStyle;
	posterSize: number;
	gridGap: number;
	showBadge: boolean;
	showRating: boolean;
	showProgress: boolean;
	showYear: boolean;

	heroStyle: HeroStyle;
	followPoster: boolean;

	font: FontName;
	textScale: number;
	radiusScale: number;
	blur: number;
	animations: AnimationLevel;

	spoilerProtection: boolean;
	hideAnimation: boolean;

	defaultStatus: Status;
	autoDates: boolean;
	autoComplete: boolean;

	landingPage: string;
	homeRows: string[];

	dateStyle: 'friendly' | 'iso' | 'long';
	clock24: boolean;

	customCss: string;
	/** Which release the "what's new" dot was last cleared for. */
	lastSeenVersion: string;
}

export const DEFAULTS: Settings = {
	theme: 'pond',
	mascot: 'classic',
	customTheme: null,
	savedThemes: [],

	scoreFormat: 'pt10dec',
	halfStars: true,

	cardStyle: 'poster',
	posterSize: 132,
	gridGap: 16,
	showBadge: true,
	showRating: true,
	showProgress: true,
	showYear: true,

	heroStyle: 'backdrop',
	followPoster: true,

	font: 'system',
	textScale: 1,
	radiusScale: 1,
	blur: 14,
	animations: 'full',

	spoilerProtection: false,
	hideAnimation: false,

	defaultStatus: 'planned',
	autoDates: true,
	autoComplete: true,

	landingPage: '/',
	homeRows: ['continue', 'upnext', 'thisweek', 'recent', 'favourites'],

	dateStyle: 'friendly',
	clock24: true,

	customCss: '',
	lastSeenVersion: ''
};

export const HOME_ROWS: { id: string; label: string; hint: string }[] = [
	{ id: 'continue', label: 'Continue watching', hint: 'Shows you are part-way through' },
	{ id: 'upnext', label: 'Up next', hint: 'The exact next episode' },
	{ id: 'thisweek', label: 'Airing this week', hint: 'Episodes due in the next seven days' },
	{ id: 'recent', label: 'Recently added', hint: 'The newest things in your library' },
	{ id: 'favourites', label: 'Favourites', hint: 'Everything you hearted' }
];

const KEY = 'frog.settings.v2';

function readLocal(): Partial<Settings> {
	if (!browser) return {};
	try {
		return JSON.parse(localStorage.getItem(KEY) ?? '{}') as Partial<Settings>;
	} catch {
		return {};
	}
}

/**
 * Preferences live on the server so a phone and a laptop agree, with a local
 * copy kept purely so the first paint after a reload is not the default theme
 * flashing before the fetch lands.
 */
class SettingsStore {
	current = $state<Settings>({ ...DEFAULTS, ...readLocal() });
	loaded = $state(false);

	private timer: ReturnType<typeof setTimeout> | null = null;
	private dirty = false;
	private flushInstalled = false;

	set<K extends keyof Settings>(key: K, value: Settings[K]) {
		this.current = { ...this.current, [key]: value };
		this.afterChange();
	}

	patch(values: Partial<Settings>) {
		this.current = { ...this.current, ...values };
		this.afterChange();
	}

	replace(values: Settings) {
		this.current = { ...DEFAULTS, ...values };
		this.afterChange();
	}

	reset() {
		this.current = { ...DEFAULTS };
		this.afterChange();
	}

	/** Pull the server copy once a session is known to exist. */
	async load() {
		if (!browser) return;
		try {
			const res = await fetch('/api/settings', { credentials: 'same-origin' });
			if (res.ok) {
				const remote = (await res.json()) as Partial<Settings>;
				if (remote && typeof remote === 'object' && Object.keys(remote).length) {
					this.current = { ...DEFAULTS, ...remote };
				}
			}
		} catch {
			// Offline: the local copy is already applied.
		} finally {
			this.loaded = true;
			this.cacheLocally();
			this.apply();
			this.installFlush();
		}
	}

	private afterChange() {
		this.cacheLocally();
		this.apply();
		this.scheduleSave();
	}

	private cacheLocally() {
		if (!browser) return;
		try {
			localStorage.setItem(KEY, JSON.stringify(this.current));
		} catch {
			// Private mode or a full quota: not worth surfacing.
		}
	}

	private scheduleSave() {
		if (!browser) return;
		if (this.timer) clearTimeout(this.timer);
		this.dirty = true;
		// Dragging a slider should not be one request per pixel, but a short
		// window keeps a quick reload from losing the change.
		this.timer = setTimeout(() => void this.save(), 250);
	}

	/**
	 * Flush on the way out. Without this, changing a setting and immediately
	 * reloading loses it: the page goes away mid-debounce, and the next load
	 * pulls the older server copy back over the top.
	 */
	private installFlush() {
		if (!browser || this.flushInstalled) return;
		this.flushInstalled = true;
		const flush = () => {
			if (!this.dirty) return;
			try {
				fetch('/api/settings', {
					method: 'PUT',
					credentials: 'same-origin',
					headers: { 'content-type': 'application/json' },
					body: JSON.stringify(this.current),
					keepalive: true
				});
				this.dirty = false;
			} catch {
				// Nothing useful to do while the page is being torn down.
			}
		};
		addEventListener('pagehide', flush);
		addEventListener('visibilitychange', () => {
			if (document.visibilityState === 'hidden') flush();
		});
	}

	async save() {
		if (!browser) return;
		try {
			await fetch('/api/settings', {
				method: 'PUT',
				credentials: 'same-origin',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify(this.current)
			});
			this.dirty = false;
		} catch {
			// The local copy still holds; it will sync on the next change.
		}
	}

	/** Write every token the CSS reads onto the document element. */
	apply() {
		if (!browser) return;
		const s = this.current;
		const root = document.documentElement;

		root.dataset.theme = s.theme;
		root.dataset.animations = s.animations;
		root.dataset.blur = s.blur === 0 ? '0' : '';
		root.dataset.card = s.cardStyle;

		const style = root.style;
		style.setProperty('--poster-min', `${s.posterSize}px`);
		style.setProperty('--gap', `${s.gridGap}px`);
		style.setProperty('--blur', `${s.blur}px`);
		style.setProperty('--font', FONTS.find((f) => f.id === s.font)?.stack ?? FONTS[0].stack);
		style.setProperty('--radius', `${Math.round(16 * s.radiusScale)}px`);
		style.setProperty('--radius-sm', `${Math.round(10 * s.radiusScale)}px`);
		style.setProperty('--radius-lg', `${Math.round(24 * s.radiusScale)}px`);
		style.fontSize = `${Math.round(16 * s.textScale)}px`;

		// A custom theme is just token overrides on top of the chosen base.
		for (const token of ['bg', 'bg-elev', 'bg-elev-2', 'accent', 'text'] as const) {
			const value = s.customTheme?.tokens?.[token];
			if (value) style.setProperty(`--${token}`, value);
			else style.removeProperty(`--${token}`);
		}

		applyCustomCss(s.customCss);
		applyFavicon(s.mascot);
	}
}

function applyCustomCss(css: string) {
	const id = 'frog-custom-css';
	let tag = document.getElementById(id) as HTMLStyleElement | null;
	if (!css.trim()) {
		tag?.remove();
		return;
	}
	if (!tag) {
		tag = document.createElement('style');
		tag.id = id;
		document.head.appendChild(tag);
	}
	tag.textContent = css;
}

function applyFavicon(mascot: MascotName) {
	const href = `/mascots/${mascot}.svg`;
	let icon = document.querySelector<HTMLLinkElement>('link[rel="icon"]');
	if (!icon) {
		icon = document.createElement('link');
		icon.rel = 'icon';
		document.head.appendChild(icon);
	}
	if (icon.getAttribute('href') !== href) {
		icon.type = 'image/svg+xml';
		icon.href = href;
	}

	// The installed app keeps whatever icon it was installed with; swapping
	// the manifest only affects the next install.
	const manifest = document.querySelector<HTMLLinkElement>('link[rel="manifest"]');
	const target = mascot === 'classic' ? '/manifest.webmanifest' : `/manifest-${mascot}.webmanifest`;
	if (manifest && manifest.getAttribute('href') !== target) manifest.href = target;
}

export const settings = new SettingsStore();
