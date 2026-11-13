import { browser } from '$app/env';
import { chooseZone } from './day';
import type { ScoreFormat } from './format';
import type { Status } from './types';

export type ThemeName =
	'glass' | 'glass-light' | 'pond' | 'pond-light' | 'dark' | 'amoled' | 'light';
export type MascotName =
	'classic' | 'sleepy' | 'popcorn' | 'cinema' | 'starry' | 'bow' | 'delighted' | 'sprout';
export type CardStyle = 'poster' | 'landscape' | 'list' | 'compact';
export type HeroStyle = 'backdrop' | 'poster' | 'off';

/**
 * Every appearance is the same glass in its own colours (`app.css`). `tone`
 * says whether it is drawn light or dark, for the few places that change with
 * that: a pale poster colour, a logo's halo.
 */
export const THEMES: {
	id: ThemeName;
	label: string;
	hint: string;
	tone: 'dark' | 'light';
	swatches: string[];
}[] = [
	{
		id: 'glass',
		label: 'Glass',
		hint: 'The default',
		tone: 'dark',
		swatches: ['#05060b', '#2a3b6e', '#3d9bff']
	},
	{
		id: 'glass-light',
		label: 'Glass Light',
		hint: 'Frosted, in daylight',
		tone: 'light',
		swatches: ['#eef1f7', '#ffffff', '#0a6fe0']
	},
	{
		id: 'pond',
		label: 'Pond',
		hint: 'Built from the logo',
		tone: 'dark',
		swatches: ['#07120e', '#16302a', '#6cc46b']
	},
	{
		id: 'pond-light',
		label: 'Pond Light',
		hint: 'Cream and green',
		tone: 'light',
		swatches: ['#f1eee1', '#fbf9f1', '#2a7247']
	},
	{
		id: 'dark',
		label: 'Dark',
		hint: 'Deep charcoal',
		tone: 'dark',
		swatches: ['#08080c', '#191922', '#7a5cff']
	},
	{
		id: 'amoled',
		label: 'AMOLED',
		hint: 'True black',
		tone: 'dark',
		swatches: ['#000000', '#121217', '#7a5cff']
	},
	{
		id: 'light',
		label: 'Light',
		hint: 'Paper',
		tone: 'light',
		swatches: ['#f6f5f8', '#ffffff', '#5b3fd6']
	}
];

export const MASCOTS: { id: MascotName; label: string; hint: string }[] = [
	{ id: 'classic', label: 'Classic', hint: 'Wide awake' },
	{ id: 'sleepy', label: 'Sleepy', hint: 'Just one more episode' },
	{ id: 'popcorn', label: 'Popcorn', hint: 'Settled in for a double bill' },
	{ id: 'cinema', label: '3D glasses', hint: 'Front row, centre' },
	{ id: 'starry', label: 'Starry-eyed', hint: 'Big eyes for big moments' },
	{ id: 'bow', label: 'Bow', hint: 'Dressed up for the premiere' },
	{ id: 'delighted', label: 'Delighted', hint: 'Best episode yet' },
	{ id: 'sprout', label: 'Sprout', hint: 'Freshly picked' }
];

export interface Settings {
	theme: ThemeName;
	mascot: MascotName;

	scoreFormat: ScoreFormat;
	halfStars: boolean;

	cardStyle: CardStyle;
	posterSize: number;
	showBadge: boolean;
	showRating: boolean;
	showProgress: boolean;
	showYear: boolean;

	heroStyle: HeroStyle;
	followPoster: boolean;

	spoilerProtection: boolean;
	hideAnimation: boolean;
	/** Episodes as cards with their stills, or as a compact list. */
	episodeView?: 'grid' | 'list';

	defaultStatus: Status;
	autoComplete: boolean;

	landingPage: string;
	homeRows: string[];
	/** Row ids this layout has been offered, so a new row appears once. */
	homeRowsSeen?: string[];
	/** Shown the welcome once. Absent on every account from before it. */
	onboarded?: boolean;

	dateStyle: 'friendly' | 'iso' | 'long';
	clock24: boolean;
	/** The IANA zone days and times are shown in; empty follows the device. */
	timeZone?: string;

	/** Smaller artwork everywhere, for mobile data. */
	dataSaver: boolean;
	/** Every page, in the order the navigation shows them. */
	navOrder: string[];
	/** Pages left out of the desktop menu bar. */
	navHidden: string[];
	/** Up to four pages in the phone tab bar; the rest live under More. */
	navTabs: string[];
	/** Which release the "what's new" dot was last cleared for. */
	lastSeenVersion: string;

	/** A status called something else, everywhere it is named. */
	statusNames?: Partial<Record<Status, string>>;
	/** A status in another colour, a #rrggbb, on every theme. */
	statusColors?: Partial<Record<Status, string>>;
	/** The letter after G that goes to a page, by its address. */
	goKeys?: Record<string, string>;
	/** A title's trailer plays without sound behind its page. */
	trailerAutoplay?: boolean;
	/** A shelf's own poster size, by shelf id, over `posterSize`. */
	shelfPosterSize?: Record<string, number>;
}

/** The letter after G for each page, until changed in Settings. */
export const GO_KEYS: Record<string, string> = {
	'/': 'h',
	'/library': 'l',
	'/search': 's',
	'/stats': 't',
	'/settings': 'c'
};

export const DEFAULTS: Settings = {
	theme: 'glass',
	mascot: 'classic',

	scoreFormat: 'pt10dec',
	halfStars: true,

	cardStyle: 'poster',
	posterSize: 132,
	showBadge: true,
	showRating: true,
	showProgress: true,
	showYear: true,

	heroStyle: 'backdrop',
	followPoster: true,

	spoilerProtection: false,
	hideAnimation: false,

	defaultStatus: 'planned',
	autoComplete: true,

	landingPage: '/',
	homeRows: ['continue', 'thisweek', 'activity', 'because'],

	dateStyle: 'friendly',
	clock24: true,

	dataSaver: false,
	navOrder: [
		'/',
		'/library',
		'/shelves',
		'/search',
		'/calendar',
		'/airing',
		'/diary',
		'/stats',
		'/settings'
	],
	navHidden: [],
	navTabs: ['/', '/library', '/search', '/calendar'],
	lastSeenVersion: ''
};

export const NAV_PAGES: { href: string; label: string; icon: string; fixed?: boolean }[] = [
	{ href: '/', label: 'Home', icon: 'home', fixed: true },
	{ href: '/library', label: 'Library', icon: 'grid' },
	{ href: '/shelves', label: 'Shelves', icon: 'layers' },
	{ href: '/search', label: 'Search', icon: 'search' },
	{ href: '/calendar', label: 'Calendar', icon: 'calendar' },
	{ href: '/airing', label: 'Airing', icon: 'tv' },
	{ href: '/diary', label: 'Diary', icon: 'book' },
	{ href: '/stats', label: 'Stats', icon: 'chart' },
	{ href: '/settings', label: 'Settings', icon: 'settings', fixed: true }
];

/** Most pages a phone tab bar holds before the More tab. */
export const MAX_TABS = 4;

/** Home as a new account has it: one idea a row, shelves first. */
export const HOME_LAYOUT = ['shelves', 'continue', 'thisweek', 'activity', 'because'];

export const HOME_ROWS: { id: string; label: string; hint: string }[] = [
	{ id: 'shelves', label: 'Your shelves', hint: 'Every shelf, one tap away' },
	{
		id: 'continue',
		label: 'Continue watching',
		hint: 'Shows you are part-way through, then those waiting on an episode'
	},
	{ id: 'thisweek', label: 'Airing this week', hint: 'Episodes due in the next seven days' },
	{ id: 'activity', label: 'Activity', hint: 'What everyone on this Frog has been watching' },
	{ id: 'because', label: 'Because you watched', hint: 'More like what you rated highly' },
	{ id: 'recent', label: 'Recently added', hint: 'The newest things in your library' },
	{ id: 'favourites', label: 'Favourites', hint: 'Everything you hearted' }
];

const KEY = 'frog.settings.v2';

/** The rows every layout saved before `homeRowsSeen` existed already knew. */
const FIRST_ROWS = ['continue', 'upnext', 'thisweek', 'recent', 'favourites'];

/**
 * Give a saved layout the rows added since it was saved, once, just after
 * "Airing this week". A row turned off afterwards stays off.
 */
function introduceRows(s: Settings): Settings {
	const seen = s.homeRowsSeen ?? FIRST_ROWS;
	const all = HOME_ROWS.map((r) => r.id);
	const fresh = all.filter((id) => !seen.includes(id));
	// Shelves have the one row at the top; a layout may still name single ones.
	const rows = [...new Set(s.homeRows)].filter((id) => !id.startsWith('shelf:'));
	if (!fresh.length && s.homeRowsSeen) return { ...s, homeRows: rows };
	for (const id of fresh) {
		if (rows.includes(id)) continue;
		// Shelves lead: they are how you get to what you put together.
		const after = id === 'shelves' ? -1 : rows.indexOf('thisweek');
		rows.splice(after >= 0 ? after + 1 : id === 'shelves' ? 0 : rows.length, 0, id);
	}
	return { ...s, homeRows: rows, homeRowsSeen: all };
}

/**
 * Settings Frog no longer has. Saved ones still load, these are left out,
 * and the next save leaves them behind for good: the theme editor's own
 * colours went, so whoever used them sees the theme they were built on, and
 * the type and motion choices went, so the page is drawn as designed (the
 * system's reduced motion is still followed).
 */
const RETIRED = ['customTheme', 'savedThemes', 'font', 'textScale', 'animations'];

/** Saved settings without the ones Frog no longer has. */
export function current(saved: unknown): Partial<Settings> {
	if (!saved || typeof saved !== 'object' || Array.isArray(saved)) return {};
	const kept = { ...(saved as Record<string, unknown>) };
	for (const key of RETIRED) delete kept[key];
	return kept as Partial<Settings>;
}

function readLocal(): Partial<Settings> {
	if (!browser) return {};
	try {
		return current(JSON.parse(localStorage.getItem(KEY) ?? '{}'));
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
	current = $state<Settings>(introduceRows({ ...DEFAULTS, ...readLocal() }));
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
		this.current = { ...DEFAULTS, ...current(values) };
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
				// An account that has never saved anything starts from the
				// defaults, not from whatever this device last showed.
				this.current = introduceRows({ ...DEFAULTS, ...current(await res.json()) });
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

	/** Save anything still waiting, while the session that owns it lasts. */
	async flush() {
		if (this.timer) clearTimeout(this.timer);
		this.timer = null;
		if (this.dirty) await this.save();
		this.dirty = false;
	}

	/**
	 * Forget this account's preferences once it has signed out, so the next
	 * account on this device loads its own rather than inheriting these and
	 * saving them over its own. Only after the session is gone: while it
	 * lasts, an unloaded store is fetched again.
	 */
	forget() {
		this.loaded = false;
		this.current = introduceRows({ ...DEFAULTS });
		this.apply();
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
		chooseZone(this.current.timeZone);
		if (!browser) return;
		const s = this.current;
		const root = document.documentElement;

		root.dataset.theme = s.theme;
		root.dataset.tone = THEMES.find((t) => t.id === s.theme)?.tone ?? 'dark';
		root.dataset.card = s.cardStyle;

		const style = root.style;
		// Not --poster-min itself: an inline value would beat the phone size.
		style.setProperty('--poster-size', `${s.posterSize}px`);

		// A status recoloured keeps its colour whatever the theme.
		for (const status of ['watching', 'completed', 'planned', 'paused', 'dropped'] as const) {
			const value = s.statusColors?.[status];
			if (value && /^#[0-9a-f]{6}$/i.test(value)) style.setProperty(`--status-${status}`, value);
			else style.removeProperty(`--status-${status}`);
		}

		applyFavicon(s.mascot);
	}
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

/** Whether the theme in use is drawn light or dark. */
export const currentTone = (): 'light' | 'dark' =>
	THEMES.find((t) => t.id === settings.current.theme)?.tone ?? 'dark';
