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
