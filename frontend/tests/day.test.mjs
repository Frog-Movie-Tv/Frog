// Dates as a person reads them. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { localDay } from '../src/lib/day.ts';

test("today is the viewer's own day, whatever UTC says", () => {
	// Half past midnight on the first, local time.
	assert.equal(localDay(new Date(2026, 10, 1, 0, 30)), '2026-11-01');
	assert.equal(localDay(new Date(2026, 0, 9, 23, 59)), '2026-01-09');
});

import { airDay } from '../src/lib/day.ts';

test('a timed release lands on the day it is here, an untimed one on its own day', () => {
	const at = new Date(2025, 2, 21, 2, 0).toISOString();
	assert.equal(airDay('2025-03-20', at), '2025-03-21');
	assert.equal(airDay('2025-03-20', null), '2025-03-20');
	assert.equal(airDay(null, 'not a date'), null);
});

import { chooseZone, viewerZone } from '../src/lib/day.ts';

test('a chosen zone decides the day a moment falls on', () => {
	try {
		chooseZone('Europe/Brussels');
		assert.equal(viewerZone(), 'Europe/Brussels');
		// The Gilded Age: Sunday 21:00 in New York is Monday in Brussels.
		assert.equal(airDay('2026-11-15', '2026-11-16T02:00:00Z'), '2026-11-16');
		// Ranma 1/2: 00:55 on Sunday in Tokyo is still Saturday in Brussels.
		assert.equal(airDay('2026-11-15', '2026-11-14T15:55:00Z'), '2026-11-14');
		chooseZone('America/New_York');
		assert.equal(airDay('2026-11-15', '2026-11-16T02:00:00Z'), '2026-11-15');
		// Without a moment, TMDB's day stands wherever the viewer is.
		assert.equal(airDay('2026-11-15', null), '2026-11-15');
	} finally {
		chooseZone(null);
	}
});

test('a zone the browser does not know follows the device', () => {
	chooseZone('Mars/Olympus_Mons');
	assert.equal(viewerZone(), Intl.DateTimeFormat().resolvedOptions().timeZone);
	chooseZone(null);
});

import { daysFrom } from '../src/lib/day.ts';

test('today and yesterday are calendar days here, not 24-hour windows', () => {
	try {
		chooseZone('Europe/Brussels');
		const now = new Date('2026-11-16T18:30:00Z'); // 19:30 on Monday in Brussels
		// 22:06 on Sunday night is yesterday, though under 24 hours ago.
		assert.equal(daysFrom('2026-11-15T21:06:50Z', now), -1);
		// 00:30 on Monday is today.
		assert.equal(daysFrom('2026-11-15T23:30:00Z', now), 0);
		assert.equal(daysFrom('2026-11-14T15:38:26Z', now), -2);
		// A bare date is that day, wherever the viewer is.
		assert.equal(daysFrom('2026-11-21', now), 5);
		// Over the clocks going back (25 October), still whole days.
		assert.equal(daysFrom('2026-10-24T12:00:00Z', new Date('2026-10-26T12:00:00Z')), -2);
		chooseZone('America/Los_Angeles');
		assert.equal(daysFrom('2026-11-21', now), 5);
		assert.equal(daysFrom('2026-11-16T07:00:00Z', now), -1); // 23:00 Sunday there
		assert.equal(daysFrom('not a date', now), null);
	} finally {
		chooseZone(null);
	}
});
