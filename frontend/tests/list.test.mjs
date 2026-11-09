// A list pasted onto a shelf. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { parseList } from '../src/lib/import/list.ts';

const brief = (rows) => rows.map((r) => [r.title, r.year, r.kind, r.tmdb_id, r.imdb_id]);

test('a line a title, years in any of the usual places, numbering dropped', () => {
	const rows = parseList(
		[
			'1. Heat (1995)',
			'2) Dune, 2021',
			'- Alien - 1979',
			'* Paprika',
			'',
			'2001: A Space Odyssey'
		].join('\n')
	);
	assert.deepEqual(brief(rows), [
		['Heat', 1995, null, null, null],
		['Dune', 2021, null, null, null],
		['Alien', 1979, null, null, null],
		['Paprika', null, null, null, null],
		['2001: A Space Odyssey', null, null, null, null]
	]);
});

test('a name that is a year stays a name', () => {
	assert.deepEqual(brief(parseList('1917')), [['1917', null, null, null, null]]);
	assert.deepEqual(brief(parseList('1917 (2019)')), [['1917', 2019, null, null, null]]);
});

test('links and ids find the title itself', () => {
	const rows = parseList(
		[
			'https://www.themoviedb.org/tv/1399-game-of-thrones',
			'https://www.imdb.com/title/tt0113277/',
			'https://letterboxd.com/film/dune-2021/',
			'https://anilist.co/manga/30002/Berserk'
		].join('\n')
	);
	assert.deepEqual(brief(rows.slice(0, 3)), [
		['', null, 'tv', 1399, null],
		['', null, null, null, 'tt0113277'],
		['dune', 2021, 'movie', null, null]
	]);
	assert.equal(rows[3].anilist_id, 30002);
	assert.equal(rows[3].kind, 'manga');
});

test('a number in a Letterboxd address is a year only when it could be one', () => {
	assert.deepEqual(brief(parseList('https://letterboxd.com/film/blade-runner-2049/')), [
		['blade runner 2049', null, 'movie', null, null]
	]);
});

test('a Letterboxd list export reads its Name and Year columns, in order', () => {
	const csv = [
		'Position,Name,Year,URL,Description',
		'1,Heat,1995,https://boxd.it/2bfI,',
		'2,"Crouching Tiger, Hidden Dragon",2000,https://boxd.it/1wvq,'
	].join('\n');
	assert.deepEqual(brief(parseList(csv)), [
		['Heat', 1995, null, null, null],
		['Crouching Tiger, Hidden Dragon', 2000, null, null, null]
	]);
});
