// Dates as a person reads them. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { localDay } from '../src/lib/day.ts';

test("today is the viewer's own day, whatever UTC says", () => {
	// Half past midnight on the first, local time.
	assert.equal(localDay(new Date(2026, 10, 1, 0, 30)), '2026-11-01');
	assert.equal(localDay(new Date(2026, 0, 9, 23, 59)), '2026-01-09');
});
