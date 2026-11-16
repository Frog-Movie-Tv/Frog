// A ladder shows only its highest step beside a name. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { topSteps } from '../src/lib/badge-ladder.ts';

test('the highest step of a ladder takes the place of the first one held', () => {
	const held = [
		{ id: 1, ladder: 'films', step: 0 },
		{ id: 9, ladder: null, step: null },
		{ id: 2, ladder: 'films', step: 2 },
		{ id: 3, ladder: 'episodes', step: 0 },
		{ id: 4, ladder: 'films', step: 1 }
	];
	assert.deepEqual(
		topSteps(held).map((b) => b.id),
		[2, 9, 3]
	);
});

test('badges made by hand are left alone', () => {
	const held = [{ id: 1 }, { id: 2 }];
	assert.deepEqual(topSteps(held), held);
});
