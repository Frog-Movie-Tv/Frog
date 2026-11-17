// The words and numbers of an event: the countdown, the lines, whether the
// premiere plays. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
	afterLine,
	caption,
	clockParts,
	inDays,
	label,
	readyLine,
	shouldPlay
} from '../src/lib/events.ts';

const readiness = (fields) => ({
	essentials_seen: 7,
	essentials: 12,
	optional_seen: 1,
	optional: 6,
	minutes_left: 875,
	per_day: 28,
	next: { title_id: 4, name: 'Loki', code: 'S2E3' },
	ready: false,
	ready_by_zero: false,
	...fields
});

test('the clock counts down and never below zero', () => {
	assert.deepEqual(clockParts(((2 * 24 + 3) * 3600 + 4 * 60 + 5) * 1000 + 999), {
		days: 2,
		hours: 3,
		minutes: 4,
		seconds: 5
	});
	assert.deepEqual(clockParts(-5000), { days: 0, hours: 0, minutes: 0, seconds: 0 });
});

test('the label says where and when, then that it is out', () => {
	assert.equal(
		label({ opens: 'cinemas', day: '2026-12-16' }, false),
		'Event · In cinemas 16 December'
	);
	assert.equal(
		label({ opens: 'season', day: '2026-12-16' }, false),
		'Event · New season 16 December'
	);
	assert.equal(label({ opens: 'cinemas', day: '2026-12-16' }, true), 'Out now · In cinemas');
});

test('before zero the line is how far along and the pace', () => {
	assert.deepEqual(readyLine(readiness()), {
		strong: '7 of 12 essentials',
		rest: '14h 35m to go · about 28 minutes a day'
	});
	assert.deepEqual(readyLine(readiness({ essentials_seen: 12, ready: true })), {
		strong: "You're ready",
		rest: 'All 12 essentials seen'
	});
});

test('after zero the line says whether you made it', () => {
	assert.equal(
		afterLine(readiness({ ready: true, ready_by_zero: true })).strong,
		'You were ready.'
	);
	assert.equal(
		afterLine(readiness({ ready: true, ready_by_zero: true })).rest,
		'12 of 12 before opening night.'
	);
	assert.deepEqual(afterLine(readiness({ essentials_seen: 8 })), {
		strong: '4 essentials left.',
		rest: "There's still time."
	});
	assert.equal(afterLine(readiness({ essentials_seen: 12, ready: true })).strong, 'All caught up.');
});

test('the premiere plays once, after zero, before the event is over', () => {
	const now = Date.parse('2026-12-16T09:00:00Z');
	const e = {
		zero: '2026-12-15T23:00:00Z',
		ends_at: '2026-12-29T23:00:00Z',
		seen: false,
		hidden: false
	};
	assert.equal(shouldPlay(e, now), true);
	assert.equal(shouldPlay({ ...e, seen: true }, now), false);
	assert.equal(shouldPlay({ ...e, hidden: true }, now), false);
	assert.equal(shouldPlay(e, Date.parse('2026-12-15T22:59:59Z')), false, 'not before zero');
	assert.equal(shouldPlay(e, Date.parse('2026-12-30T00:00:00Z')), false, 'never once it is past');
});

test('days are counted the way people say them', () => {
	const now = Date.parse('2026-12-10T12:00:00Z');
	assert.equal(inDays('2026-12-10T23:00:00Z', now), 'Tomorrow');
	assert.equal(inDays('2026-12-13T23:00:00Z', now), 'In 4 days');
	assert.equal(inDays('2026-12-10T11:00:00Z', now), 'Today');
});

test('a card on the Before row says what is left of it', () => {
	const entry = (standing, fields = {}) => ({
		item: { kind: 'tv', runtime: null },
		essential: true,
		standing: { seen: false, watched: 0, total: 6, minutes_left: 270, next: null, ...standing },
		...fields
	});
	assert.equal(caption(entry({ seen: true })), 'Seen');
	assert.equal(
		caption(entry({ watched: 2, next: 'S1E3', minutes_left: 180 })),
		'Next S1E3 · 3h left'
	);
	assert.equal(caption(entry({}, { essential: false })), 'Optional');
	assert.equal(caption(entry({})), '4h 30m');
});
