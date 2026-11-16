/*
 * Days and zones as the person looking at them has them. The zone is the one
 * chosen in Settings, else this device's; the settings store passes the
 * choice in (`chooseZone`), so these stay plain functions that tests can run.
 */

let chosen: string | null = null;

/** Whether the browser knows this IANA zone. */
export function isZone(zone: string): boolean {
	try {
		new Intl.DateTimeFormat('en', { timeZone: zone });
		return true;
	} catch {
		return false;
	}
}

/** This device's zone. */
export function deviceZone(): string {
	try {
		return Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';
	} catch {
		return 'UTC';
	}
}

/** A chosen zone when it is one, else this device's. */
export function zoneFor(zone: string | null | undefined): string {
	return zone && isZone(zone) ? zone : deviceZone();
}

/** The zone chosen in Settings; empty follows the device. */
export function chooseZone(zone: string | null | undefined): void {
	chosen = zone && isZone(zone) ? zone : null;
}

/** The zone days and times are shown in. */
export function viewerZone(): string {
	return chosen ?? deviceZone();
}

const dayFormats = new Map<string, Intl.DateTimeFormat>();

/** The day as the person looking at it would write it, not as UTC has it. */
export function localDay(at: Date = new Date()): string {
	const zone = viewerZone();
	let format = dayFormats.get(zone);
	if (!format) {
		format = new Intl.DateTimeFormat('en-CA', {
			timeZone: zone,
			year: 'numeric',
			month: '2-digit',
			day: '2-digit'
		});
		dayFormats.set(zone, format);
	}
	const parts = format.formatToParts(at);
	const part = (type: string) => parts.find((p) => p.type === type)?.value ?? '';
	return `${part('year')}-${part('month')}-${part('day')}`;
}

/**
 * How many of the viewer's calendar days lie between today and a moment:
 * 0 is today, -1 yesterday, 1 tomorrow. A bare date ("2026-11-21") is that
 * day wherever the viewer is, not UTC's midnight moved into their zone.
 */
export function daysFrom(at: string | Date, now: Date = new Date()): number | null {
	let day: string;
	if (typeof at === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(at)) day = at;
	else {
		const moment = typeof at === 'string' ? new Date(at) : at;
		if (Number.isNaN(moment.getTime())) return null;
		day = localDay(moment);
	}
	// Both as UTC midnights, so a change of clocks never makes a day 23 hours.
	const diff = Date.parse(`${day}T00:00:00Z`) - Date.parse(`${localDay(now)}T00:00:00Z`);
	return Math.round(diff / 86_400_000);
}

/**
 * The day something comes out, where the viewer is. A moment from AniList or
 * TVmaze (`air_at`) can fall on the day before or after TMDB's date; without
 * one, TMDB's day stands.
 */
export function airDay(date: string | null, at?: string | null): string | null {
	if (!at) return date;
	const moment = new Date(at);
	return Number.isNaN(moment.getTime()) ? date : localDay(moment);
}
