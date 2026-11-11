// How a score is entered: the slider's scale for the numbers, and the stars
// a touch fills. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { marks, slideScale, sliding, snap, starStep, starsAt } from '../src/lib/score-entry.ts';

test('every number format slides; stars and smileys do not', () => {
	assert.equal(sliding('pt100'), true);
	assert.equal(sliding('pt10dec'), true);
	assert.equal(sliding('pt10'), true);
	assert.equal(sliding('star5'), false);
	assert.equal(sliding('smiley3'), false);
	assert.deepEqual(slideScale('pt100'), { max: 100, step: 1 });
	assert.deepEqual(slideScale('pt10dec'), { max: 10, step: 0.1 });
	assert.deepEqual(slideScale('pt10'), { max: 10, step: 1 });
});

test('ten points stays whole, from 0 to 10', () => {
	assert.equal(snap(7.4, 'pt10'), 7);
	assert.equal(snap(7.6, 'pt10'), 8);
	assert.equal(snap(12, 'pt10'), 10);
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

test('a touch fills the stars up to where it lands', () => {
	// Five stars across: each is a fifth of the row, each half a tenth.
	assert.equal(starsAt(0.05, true), 0.5, 'the left half of the first star');
	assert.equal(starsAt(0.15, true), 1);
	assert.equal(starsAt(0.65, true), 3.5);
	assert.equal(starsAt(0.7, true), 3.5, 'exactly on a half stays on it');
	assert.equal(starsAt(0.71, true), 4);
	assert.equal(starsAt(1, true), 5);
	assert.equal(starsAt(1.4, true), 5, 'past the end is all five');
	assert.equal(starsAt(-1, true), 0, 'before the first star is zero stars');
	assert.equal(starsAt(-0.01, false), 0);
	assert.equal(starsAt(Number.NaN, true), 0.5);
});

test('without halves any part of a star is the whole star', () => {
	assert.equal(starsAt(0.05, false), 1);
	assert.equal(starsAt(0.65, false), 4);
	assert.equal(starsAt(0.6, false), 3);
});

test('arrow keys move by a half or a whole, within the row', () => {
	assert.equal(starStep(3, 1, true), 3.5);
	assert.equal(starStep(3, -1, false), 2);
	assert.equal(starStep(5, 1, true), 5);
	assert.equal(starStep(0, -1, true), 0, 'zero stars is as low as it goes');
	assert.equal(starStep(0.5, -1, true), 0);
	assert.equal(starStep(2.5, 1, false), 4, 'a half star rounds to the scale first');
});
