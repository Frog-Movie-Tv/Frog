/**
 * The badges page: every badge as one card, earned or still to come, and the
 * ways to narrow and order them. Pure, so `tests/badge-board.test.mjs` can
 * drive it without a page.
 */

import type { BadgeBoard, BadgeCondition, BadgeGroup, HeldBadge, LockedBadge } from './types';

export interface BadgeCard {
	id: number;
	name: string;
	description: string | null;
	small: string;
	large: string;
	earned: boolean;
	group: BadgeGroup;
	moment: boolean;
	/** 1 (easy) to 5 (legendary), or null for a badge given by hand. */
	difficulty: number | null;
	ladder: string | null;
	/** 0 for a ladder's first step. */
	step: number | null;
	steps: number | null;
	holders: number;
	/** The day it was earned or given (YYYY-MM-DD), when it is held. */
	day: string | null;
	held: HeldBadge | null;
	progress: { have: number; need: number } | null;
	/** For one made here: what earns it on its own. */
	condition: BadgeCondition | null;
}

export const GROUPS: { id: BadgeGroup; label: string; about: string }[] = [
	{ id: 'watching', label: 'Films and shows', about: 'How much you watch.' },
	{ id: 'moments', label: 'Moments', about: 'Only what you log as it happens counts.' },
	{ id: 'taste', label: 'Taste', about: 'What you watch and how you rate it.' },
	{ id: 'anime', label: 'Anime and manga', about: 'Episodes, films and chapters.' },
	{ id: 'events', label: 'Events', about: 'Ready before the big ones, and there when they open.' },
	{ id: 'frog', label: 'Your Frog', about: 'Your library, your shelves and your people.' },
	{ id: 'collector', label: 'Collector', about: 'Badges for badges.' },
	{ id: 'custom', label: 'Made on this Frog', about: 'Made here, earned like any other.' },
	{ id: 'given', label: 'Given by hand', about: 'From the people who run this Frog.' }
];

export const DIFFICULTY = ['Easy', 'Medium', 'Hard', 'Very hard', 'Legendary'];

/** What a ladder counts, for "Films, step 3 of 10". */
export const LADDERS: Record<string, string> = {
	films: 'Films',
	episodes: 'Episodes',
	hours: 'Hours watched',
	library: 'Library',
	completed: 'Shows finished',
	ratings: 'Ratings',
	chapters: 'Chapters',
	manga: 'Manga finished',
	streak: 'Streaks',
	rewatches: 'Rewatches',
	extension: 'Browser extension',
	favourites: 'Favourites',
	years: 'Years on Frog',
	together: 'Watched together',
	anime: 'Anime',
	seasons: 'Seasons finished',
	days: 'Days watched',
	notes: 'Notes'
};

/**
 * What a badge made here can count, as the server allows it
 * (`awards::custom::COUNTS`), and how a number of it reads.
 */
export const COUNTS: { id: string; label: string; reads: (n: string) => string }[] = [
	{ id: 'films', label: 'Films watched', reads: (n) => `${n} films` },
	{ id: 'episodes', label: 'Episodes watched', reads: (n) => `${n} episodes` },
	{ id: 'hours', label: 'Hours watched', reads: (n) => `${n} hours watched` },
	{
		id: 'days',
		label: 'Days with something watched',
		reads: (n) => `Something watched on ${n} days`
	},
	{ id: 'seasons', label: 'Seasons finished', reads: (n) => `${n} seasons finished` },
	{ id: 'completed', label: 'Shows finished', reads: (n) => `${n} shows finished` },
	{ id: 'rewatches', label: 'Rewatches', reads: (n) => `${n} rewatches` },
	{ id: 'library', label: 'Titles in the library', reads: (n) => `${n} titles in your library` },
	{ id: 'ratings', label: 'Titles rated', reads: (n) => `${n} titles rated` },
	{ id: 'favourites', label: 'Favourites', reads: (n) => `${n} favourites` },
	{ id: 'notes', label: 'Titles with notes', reads: (n) => `Notes on ${n} titles` },
	{ id: 'anime', label: 'Anime watched', reads: (n) => `${n} anime` },
	{ id: 'chapters', label: 'Manga chapters read', reads: (n) => `${n} manga chapters` },
	{ id: 'manga', label: 'Manga finished', reads: (n) => `${n} manga finished` },
	{
		id: 'together',
		label: 'Watched with someone',
		reads: (n) => `${n} titles watched with someone`
	},
	{
		id: 'extension',
		label: 'Logged by the browser extension',
		reads: (n) => `${n} logged by the browser extension`
	},
	{ id: 'years', label: 'Years on Frog', reads: (n) => `${n} years on Frog` }
];

/** "25 films", or nothing for a count Frog does not know. */
export function conditionText(c: BadgeCondition | null | undefined): string {
	if (!c) return '';
	const count = COUNTS.find((x) => x.id === c.count);
	return count ? count.reads(c.at.toLocaleString()) : '';
}

export type Show = 'all' | 'earned' | 'locked';
export type Sort = 'kind' | 'recent' | 'closest' | 'easiest' | 'hardest' | 'rarest' | 'name';

export const SORTS: { value: Sort; label: string; own?: boolean }[] = [
	{ value: 'kind', label: 'By kind' },
	{ value: 'recent', label: 'Recently earned' },
	{ value: 'closest', label: 'Closest to earning', own: true },
	{ value: 'easiest', label: 'Easiest first' },
	{ value: 'hardest', label: 'Hardest first' },
	{ value: 'rarest', label: 'Rarest first' },
	{ value: 'name', label: 'A to Z' }
];

/** The day a held badge was earned, or given. */
export const heldDay = (b: HeldBadge) => b.earned_on ?? b.awarded_at.slice(0, 10);

/** Every badge on the board as a card, in the server's order: earned first. */
export function cards(board: BadgeBoard): BadgeCard[] {
	const earned = board.earned.map((b): BadgeCard => ({
		id: b.id,
		name: b.name,
		description: b.description,
		small: b.small,
		large: b.large,
		earned: true,
		group: b.group ?? (b.automatic ? 'watching' : 'given'),
		moment: b.moment ?? false,
		difficulty: b.difficulty ?? null,
		ladder: b.ladder ?? null,
		step: b.step ?? null,
		steps: b.steps ?? null,
		holders: b.holders ?? 0,
		day: heldDay(b),
		held: b,
		progress: null,
		condition: b.condition ?? null
	}));
	const locked = board.locked.map((b: LockedBadge): BadgeCard => ({
		id: b.id,
		name: b.name,
		description: b.description,
		small: b.small,
		large: b.large,
		earned: false,
		group: b.group ?? 'watching',
		moment: b.moment,
		difficulty: b.difficulty ?? null,
		ladder: b.ladder,
		step: b.step,
		steps: b.steps ?? null,
		holders: b.holders ?? 0,
		day: null,
		held: null,
		progress: b.progress,
		condition: b.condition ?? null
	}));
	return [...earned, ...locked];
}

/** How far along a card is: 1 for earned, 0 to 1 for one counted in steps. */
export function share(card: BadgeCard): number {
	if (card.earned) return 1;
	if (!card.progress || card.progress.need <= 0) return 0;
	return Math.min(1, card.progress.have / card.progress.need);
}

const fold = (text: string) =>
	text
		.normalize('NFD')
		.replace(/\p{Diacritic}/gu, '')
		.toLowerCase();

export interface Narrow {
	show: Show;
	group: BadgeGroup | 'all';
	query: string;
}

export function narrow(all: BadgeCard[], { show, group, query }: Narrow): BadgeCard[] {
	const words = fold(query).split(/\s+/).filter(Boolean);
	return all.filter((c) => {
		if (show === 'earned' && !c.earned) return false;
		if (show === 'locked' && c.earned) return false;
		if (group !== 'all' && c.group !== group) return false;
		if (!words.length) return true;
		const text = fold(
			[c.name, c.description ?? '', c.ladder ? (LADDERS[c.ladder] ?? c.ladder) : ''].join(' ')
		);
		return words.every((w) => text.includes(w));
	});
}

/**
 * Cards in the chosen order. Every order keeps the catalogue's order for
 * ties, so a ladder's steps stay bottom to top.
 */
export function sorted(list: BadgeCard[], sort: Sort): BadgeCard[] {
	const placed = list.map((c, i) => ({ c, i }));
	const hard = (c: BadgeCard) => c.difficulty ?? 3;
	const by: Record<Sort, (a: BadgeCard, b: BadgeCard) => number> = {
		kind: () => 0,
		recent: (a, b) =>
			Number(b.earned) - Number(a.earned) || (b.day ?? '').localeCompare(a.day ?? ''),
		closest: (a, b) => {
			// Earned ones last: the point is what to go for next.
			if (a.earned !== b.earned) return a.earned ? 1 : -1;
			return share(b) - share(a);
		},
		easiest: (a, b) => hard(a) - hard(b),
		hardest: (a, b) => hard(b) - hard(a),
		rarest: (a, b) => a.holders - b.holders,
		name: (a, b) => a.name.localeCompare(b.name)
	};
	return placed.sort((x, y) => by[sort](x.c, y.c) || x.i - y.i).map(({ c }) => c);
}

/** The cards in the page's sections, for the order "By kind". */
export function grouped(list: BadgeCard[]) {
	return GROUPS.map((g) => ({ ...g, cards: list.filter((c) => c.group === g.id) })).filter(
		(g) => g.cards.length
	);
}

/** The newest earned, and the nearest still to come. */
export function highlights(all: BadgeCard[]) {
	const newest = sorted(
		all.filter((c) => c.earned),
		'recent'
	)[0];
	const next = sorted(
		all.filter((c) => !c.earned && c.progress && share(c) > 0),
		'closest'
	)[0];
	return { newest: newest ?? null, next: next ?? null };
}

/** "Held by 2 of 5 people", or "Nobody here yet". */
export function heldBy(holders: number, accounts: number): string {
	if (!holders) return 'Nobody here has it yet';
	if (accounts && holders >= accounts) return 'Everyone here has it';
	return accounts
		? `Held by ${holders} of ${accounts} people here`
		: `Held by ${holders} ${holders === 1 ? 'person' : 'people'}`;
}

/** The address's filters, read back. Anything unknown falls back. */
export function fromAddress(search: { get(name: string): string | null }): Narrow & { sort: Sort } {
	const show = search.get('show');
	const sort = search.get('sort');
	const group = search.get('kind');
	return {
		show: show === 'earned' || show === 'locked' ? show : 'all',
		sort: SORTS.some((s) => s.value === sort) ? (sort as Sort) : 'kind',
		group: GROUPS.some((g) => g.id === group) ? (group as BadgeGroup) : 'all',
		query: search.get('q') ?? ''
	};
}

export function toAddress(state: Narrow & { sort: Sort }): string {
	const out = new URLSearchParams();
	if (state.show !== 'all') out.set('show', state.show);
	if (state.sort !== 'kind') out.set('sort', state.sort);
	if (state.group !== 'all') out.set('kind', state.group);
	if (state.query.trim()) out.set('q', state.query.trim());
	const text = out.toString();
	return text ? `?${text}` : '';
}
