// The slider's scale, for the formats with too many values to lay out as
// buttons. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { marks, slideScale, sliding, snap } from '../src/lib/score-entry.ts';

test('only the fine-grained formats slide', () => {
	assert.equal(sliding('pt100'), true);
	assert.equal(sliding('pt10dec'), true);
	assert.equal(sliding('pt10'), false);
	assert.equal(sliding('star5'), false);
	assert.equal(sliding('smiley3'), false);
	assert.deepEqual(slideScale('pt100'), { max: 100, step: 1 });
	assert.deepEqual(slideScale('pt10dec'), { max: 10, step: 0.1 });
});

test('a hundred points stays whole, from 0 to 100', () => {
	assert.equal(snap(87, 'pt100'), 87);
	assert.equal(snap(87.4, 'pt100'), 87);
	assert.equal(snap(87.6, 'pt100'), 88);
	assert.equal(snap(140, 'pt100'), 100);
	assert.equal(snap(-5, 'pt100'), 0);
});

test('ten with a decimal keeps one decimal, from 0 to 10', () => {
	assert.equal(snap(8.5, 'pt10dec'), 8.5);
	assert.equal(snap(8.55, 'pt10dec'), 8.6, 'two decimals round to one');
	assert.equal(snap(8.700000000000001, 'pt10dec'), 8.7, 'no floating-point tails');
	assert.equal(snap(10.4, 'pt10dec'), 10);
	assert.equal(snap(-0.5, 'pt10dec'), 0);
	assert.equal(snap(Number.NaN, 'pt10dec'), 0);
});

test('each scale has its marks', () => {
	assert.deepEqual(marks('pt100'), [0, 25, 50, 75, 100]);
	assert.deepEqual(marks('pt10dec'), [0, 2, 4, 6, 8, 10]);
});
