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
