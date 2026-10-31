// Lists from TMDB and AniList on their way to a keyed list. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { titleKey } from '../src/lib/titles.ts';
import { uniqueBy } from '../src/lib/unique.ts';

test('keeps the first of each, in order', () => {
	const people = [
		{ id: 5727, part: 'The Missing Link' },
		{ id: 1, part: 'Ginormica' },
		{ id: 5727, part: 'Coverton' }
	];
	assert.deepEqual(
		uniqueBy(people, (p) => p.id).map((p) => p.part),
		['The Missing Link', 'Ginormica']
	);
});

test('a film and a show with one TMDB id are two titles', () => {
	const list = [
		{ kind: 'movie', tmdb_id: 5 },
		{ kind: 'tv', tmdb_id: 5 }
	];
	assert.equal(uniqueBy(list, titleKey).length, 2);
});

test('two anime TMDB lacks are two titles, and one seen twice is one', () => {
	const list = [
		{ kind: 'tv', tmdb_id: null, anilist_id: 21 },
		{ kind: 'tv', tmdb_id: null, anilist_id: 22 },
		{ kind: 'tv', tmdb_id: null, anilist_id: 21 }
	];
	assert.equal(uniqueBy(list, titleKey).length, 2);
});

test('two providers naming one title of Frog are one', () => {
	const list = [
		{ id: 9, kind: 'tv', tmdb_id: 1 },
		{ id: 9, kind: 'tv', anilist_id: 21 }
	];
	assert.equal(uniqueBy(list, titleKey).length, 1);
});
