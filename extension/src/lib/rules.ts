// What the extension learns about a site when you confirm or correct a match.
//
// A rule is keyed by the site's domain, which is never written in the code:
// it is whatever site you were on. It says which source held the id, which
// held the season and episode, and which held the name, plus the names this
// site uses for titles you picked by hand. With a rule, the next match on that
// domain is exact.

import { fold, SOURCES, type Clue, type Merged, type Preference, type Source } from './signals.ts';

export interface Rule extends Preference {
	/** When it was last taught, ISO. */
	updated: string;
}

export type Rules = Record<string, Rule>;

/** A site's key: its host, without `www.` and without a port. */
export function domainOf(url: string): string | null {
	try {
		const u = new URL(url);
		if (u.protocol !== 'http:' && u.protocol !== 'https:') return null;
		return u.hostname.replace(/^www\./, '').toLowerCase();
	} catch {
		return null;
	}
}

/** What was settled on: Frog's title and, for a show, the episode. */
export interface Chosen {
	title_id: number;
	tmdb?: { kind: string; id: number } | null;
	season?: number | null;
	episode?: number | null;
}

/**
 * Teach a rule from a confirmed or corrected match: which clue held the id
 * Frog agreed with, which held the numbers that were logged, and which held
 * the name, which from now on means this title on this site.
 */
export function learn(
	previous: Rule | undefined,
	clues: Clue[],
	merged: Merged,
	chosen: Chosen
): Rule {
	const rule: Rule = {
		...previous,
		titles: { ...previous?.titles },
		updated: new Date().toISOString()
	};

	const idClue = chosen.tmdb
		? clues.find((c) => c.tmdb === chosen.tmdb!.id && (!c.kind || c.kind === chosen.tmdb!.kind))
		: undefined;
	if (idClue) rule.idFrom = idClue.source;

	if (chosen.episode != null) {
		const same = (c: Clue) =>
			(c.episode === chosen.episode && (c.season === undefined || c.season === chosen.season)) ||
			(merged.signals.absolute !== undefined &&
				c.episode === merged.signals.absolute &&
				c.season === undefined);
		const numbers =
			clues.find((c) => c.source === merged.from.numbers && same(c)) ?? clues.find(same);
		if (numbers) rule.episodeFrom = numbers.source;
	}

	// A TMDB title Frog had not opened yet has no id of Frog's to remember.
	if (merged.signals.title && merged.from.name && chosen.title_id > 0) {
		rule.nameFrom = merged.from.name;
		rule.titles![fold(merged.signals.title)] = chosen.title_id;
	}
	return rule;
}

/** The parts of a rule `merge` reads. */
export function preferenceOf(rule: Rule | undefined): Preference {
	if (!rule) return {};
	return {
		idFrom: rule.idFrom,
		episodeFrom: rule.episodeFrom,
		nameFrom: rule.nameFrom,
		titles: rule.titles
	};
}

const isSource = (v: unknown): v is Source =>
	typeof v === 'string' && SOURCES.includes(v as Source);

/** A rule as it should be, or null: what an import or a hand edit is checked against. */
export function checkRule(value: unknown): Rule | null {
	if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
	const v = value as Record<string, unknown>;
	const rule: Rule = {
		updated: typeof v.updated === 'string' ? v.updated : new Date().toISOString()
	};
	for (const key of ['idFrom', 'episodeFrom', 'nameFrom'] as const) {
		if (v[key] === undefined) continue;
		if (!isSource(v[key])) return null;
		rule[key] = v[key] as Source;
	}
	if (v.titles !== undefined) {
		if (!v.titles || typeof v.titles !== 'object' || Array.isArray(v.titles)) return null;
		const titles: Record<string, number> = {};
		for (const [name, id] of Object.entries(v.titles as Record<string, unknown>)) {
			if (typeof id !== 'number' || !Number.isInteger(id) || id <= 0) return null;
			titles[fold(name)] = id;
		}
		rule.titles = titles;
	}
	return rule;
}

const DOMAIN = /^(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)*[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/;

/** A domain as a rule is keyed, from whatever was typed, or null. */
export function cleanDomain(typed: string): string | null {
	let t = typed.trim().toLowerCase();
	if (!t) return null;
	if (t.includes('://')) t = domainOf(t) ?? '';
	t = t
		.replace(/^www\./, '')
		.replace(/\/.*$/, '')
		.replace(/:\d+$/, '');
	return DOMAIN.test(t) ? t : null;
}

/** Rules read from an export. Anything that is not a rule is left out and counted. */
export function readExport(text: string): { rules: Rules; skipped: number } {
	let parsed: unknown;
	try {
		parsed = JSON.parse(text);
	} catch {
		throw new Error('That file is not a rules export.');
	}
	const body = (parsed as { rules?: unknown })?.rules ?? parsed;
	if (!body || typeof body !== 'object' || Array.isArray(body)) {
		throw new Error('That file is not a rules export.');
	}
	const rules: Rules = {};
	let skipped = 0;
	for (const [domain, value] of Object.entries(body as Record<string, unknown>)) {
		const key = cleanDomain(domain);
		const rule = checkRule(value);
		if (key && rule) rules[key] = rule;
		else skipped++;
	}
	return { rules, skipped };
}

export function writeExport(rules: Rules): string {
	return JSON.stringify({ frog: 'site-rules', version: 1, rules }, null, 2);
}

/** A short line saying what a rule does, for the options page. */
export function describe(rule: Rule): string {
	const parts: string[] = [];
	const label: Record<Source, string> = {
		'page-url': 'the address',
		'frame-url': 'the player’s address',
		'iframe-src': 'the player’s address',
		jsonld: 'the page’s data',
		meta: 'the page’s tags',
		'media-session': 'the player',
		title: 'the page title',
		heading: 'the heading'
	};
	if (rule.idFrom) parts.push(`Title from ${label[rule.idFrom]}`);
	if (rule.episodeFrom) parts.push(`episode from ${label[rule.episodeFrom]}`);
	const names = Object.keys(rule.titles ?? {}).length;
	if (names) parts.push(`${names} ${names === 1 ? 'name' : 'names'} learned`);
	const line = parts.join(', ');
	return line ? line[0]!.toUpperCase() + line.slice(1) : 'Nothing learned yet';
}
