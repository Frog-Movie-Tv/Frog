/**
 * The words and numbers of an event: the countdown, the readiness line, the
 * label over the title, and whether the premiere should play. Pure, so the
 * hero, the pages and the tests agree (`tests/events.test.mjs`).
 */

import { runtime } from './duration';
import type { EventListItem, EventReadiness, EventSummary } from './types';

export interface ClockParts {
	days: number;
	hours: number;
	minutes: number;
	seconds: number;
}

/** What is left until a moment, never below zero. */
export function clockParts(ms: number): ClockParts {
	const total = Math.max(0, Math.floor(ms / 1000));
	return {
		days: Math.floor(total / 86_400),
		hours: Math.floor((total % 86_400) / 3600),
		minutes: Math.floor((total % 3600) / 60),
		seconds: total % 60
	};
}

export const pad = (n: number) => String(n).padStart(2, '0');

/** "16 December", from the day itself, whatever the zone. */
export function eventDay(day: string): string {
	const [y, m, d] = day.split('-').map(Number);
	if (!y || !m || !d) return day;
	return new Date(y, m - 1, d).toLocaleDateString('en-GB', { day: 'numeric', month: 'long' });
}

const OPENS = { cinemas: 'In cinemas', streaming: 'Streaming', season: 'New season' } as const;

/** The small line over the title. */
export function label(e: Pick<EventSummary, 'opens' | 'day'>, out: boolean): string {
	const where = OPENS[e.opens] ?? OPENS.cinemas;
	return out ? `Out now · ${where}` : `Event · ${where} ${eventDay(e.day)}`;
}

export interface Line {
	strong: string;
	rest: string;
}

const essentials = (n: number) => (n === 1 ? 'essential' : 'essentials');

/** Before zero: how far along, and the pace that gets there. */
export function readyLine(r: EventReadiness): Line {
	if (r.ready)
		return { strong: "You're ready", rest: `All ${r.essentials} ${essentials(r.essentials)} seen` };
	const pace = r.per_day
		? ` · about ${r.per_day} ${r.per_day === 1 ? 'minute' : 'minutes'} a day`
		: '';
	return {
		strong: `${r.essentials_seen} of ${r.essentials} ${essentials(r.essentials)}`,
		rest: `${runtime(r.minutes_left) || 'Nothing'} to go${pace}`
	};
}

/** After zero: the line the premiere ends on, and the hero keeps. */
export function afterLine(r: EventReadiness): Line {
	const left = r.essentials - r.essentials_seen;
	if (r.ready_by_zero)
		return {
			strong: 'You were ready.',
			rest: `${r.essentials} of ${r.essentials} before opening night.`
		};
	if (r.ready)
		return { strong: 'All caught up.', rest: `${r.essentials} of ${r.essentials} seen.` };
	return { strong: `${left} ${essentials(left)} left.`, rest: "There's still time." };
}

/** Whether the moment has come for this account. */
export const isOut = (e: Pick<EventSummary, 'zero'>, now: number) => now >= Date.parse(e.zero);

/** The premiere plays once per account: out, not seen, not hidden, not over. */
export function shouldPlay(
	e: Pick<EventSummary, 'zero' | 'seen' | 'hidden' | 'ends_at'>,
	now: number
): boolean {
	return !e.seen && !e.hidden && isOut(e, now) && now < Date.parse(e.ends_at);
}

export type ItemState = 'seen' | 'progress' | 'library' | 'none';

export function itemState(li: EventListItem): ItemState {
	if (li.standing.seen) return 'seen';
	if (li.standing.watched > 0) return 'progress';
	if (li.item.entry) return 'library';
	return 'none';
}

/** Under a title on the list: its year and length, or its seasons. */
export function itemFacts(li: EventListItem): string {
	const year = (li.item.release_date ?? '').slice(0, 4);
	if (li.item.kind === 'movie') return [year, runtime(li.item.runtime)].filter(Boolean).join(' · ');
	const [a, b] = li.seasons ?? [1, li.item.season_count ?? 1];
	const seasons = a === b ? `Season ${a}` : `Seasons ${a}–${b}`;
	const eps = li.standing.total ? `${li.standing.total} episodes` : '';
	return [seasons, eps].filter(Boolean).join(' · ');
}

/** A card's caption on the Before row. */
export function caption(li: EventListItem): string {
	const s = li.standing;
	if (s.seen) return 'Seen';
	if (s.watched > 0 && s.next) return `Next ${s.next} · ${runtime(s.minutes_left)} left`;
	if (!li.essential) return 'Optional';
	return runtime(s.minutes_left) || (li.item.kind === 'tv' ? 'Show' : 'Film');
}

/** Days until a moment, counted the way people say it: "In 3 days". */
export function inDays(zero: string, now: number): string {
	const days = Math.ceil((Date.parse(zero) - now) / 86_400_000);
	if (days <= 0) return 'Today';
	if (days === 1) return 'Tomorrow';
	return `In ${days} days`;
}

/** The line under an event on the Events list. */
export function listLine(e: EventSummary, now: number): string {
	const r = e.readiness;
	if (e.phase === 'scheduled')
		return `Shows on Home from ${new Date(e.shows_from).toLocaleDateString('en-GB', { day: 'numeric', month: 'long' })}`;
	if (e.phase === 'live')
		return r
			? `${inDays(e.zero, now)} · ${r.essentials_seen} of ${r.essentials} ready`
			: inDays(e.zero, now);
	const when = `${e.phase === 'past' ? 'Premiered' : 'Out since'} ${eventDay(e.day)}`;
	if (!r) return when;
	return `${when} · ${r.ready_by_zero ? 'you were ready' : `${r.essentials_seen} of ${r.essentials} seen`}`;
}
