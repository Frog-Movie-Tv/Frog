// The rating deck's rules: which titles are dealt, in what order, and what a
// card's buttons and keys offer. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { choiceForKey, choices, deckOf } from '../src/lib/rate.ts';

const item = (id, status, rating, lastWatched = null, added = '2026-01-01') => ({
	id,
	name: `Title ${id}`,
	entry: { status, rating, added_at: added },
	last_watched_at: lastWatched
});

test('the deck holds what is finished and unrated, what you watched lately first', () => {
	const items = [
		item(1, 'completed', 80),
		item(2, 'completed', null, '2026-03-01'),
		item(3, 'planned', null),
		item(4, 'watching', null, '2026-05-01'),
		item(5, 'dropped', null, null, '2026-04-01'),
		item(6, 'paused', null, null, '2026-02-01'),
		item(7, 'completed', null, '2026-04-01'),
		item(8, 'completed', null, null, '2026-06-01')
	];
	assert.deepEqual(
		deckOf(items, 'finished').map((i) => i.id),
		[7, 2, 8],
		'only finished titles; never-watched by when added'
	);
	assert.deepEqual(
		deckOf(items, 'all').map((i) => i.id),
		[4, 7, 2, 8, 5, 6, 3]
	);
});

test('a title outside the library is never dealt', () => {
	assert.deepEqual(deckOf([{ id: 9, entry: null, last_watched_at: null }], 'all'), []);
});

test('every format offers at most ten scores', () => {
	assert.deepEqual(choices('pt100', false), [10, 20, 30, 40, 50, 60, 70, 80, 90, 100]);
	assert.deepEqual(choices('pt10', false), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
	assert.deepEqual(choices('pt10dec', false), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
	assert.deepEqual(choices('star5', true), [0.5, 1, 1.5, 2, 2.5, 3, 3.5, 4, 4.5, 5]);
	assert.deepEqual(choices('star5', false), [1, 2, 3, 4, 5]);
	assert.deepEqual(choices('smiley3', false), [1, 2, 3]);
});

test('number keys pick the choices, 0 the tenth', () => {
	assert.equal(choiceForKey('1', 10), 0);
	assert.equal(choiceForKey('0', 10), 9);
	assert.equal(choiceForKey('4', 3), null, 'no fourth smiley');
	assert.equal(choiceForKey('a', 10), null);
});
