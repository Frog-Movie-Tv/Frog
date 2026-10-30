// The readers for other trackers' exports, each against a small file in that
// tracker's own shape. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { animeTitle, dedupe, parseCsv, readFile } from '../src/lib/import/parsers.ts';

/** Read one file as the import page does, and return its rows. */
async function rows(format, name, text) {
	const files = await readFile(format, new File([text], name));
	return files.flatMap((f) => f.rows);
}

/** The fields a row is judged by; the key is a counter and says nothing. */
const shape = (r) => {
	const { key, ...rest } = r;
	assert.match(key, /^row-\d+$/);
	return rest;
};

test('CSV keeps quoted commas, quotes and newlines, and drops a byte order mark', () => {
	const text =
		'﻿Name,Note\r\n"Crouching Tiger, Hidden Dragon","He said ""hi"""\n"Two\nlines",x\n\n';
	assert.deepEqual(parseCsv(text), [
		['Name', 'Note'],
		['Crouching Tiger, Hidden Dragon', 'He said "hi"'],
		['Two\nlines', 'x']
	]);
});

test('Letterboxd: diary, watchlist and likes each mean something different', async () => {
	const diary = await rows(
		'letterboxd',
		'diary.csv',
		'Date,Name,Year,Letterboxd URI,Rating,Rewatch,Tags,Watched Date\n' +
			'2024-01-02,Heat,1995,https://boxd.it/1,4.5,Yes,,2024-01-01\n'
	);
	assert.deepEqual(diary.map(shape), [
		{
			title: 'Heat',
			year: 1995,
			imdb_id: null,
			tmdb_id: null,
			kind: 'movie',
			rating: 90,
			status: 'completed',
			watched_at: '2024-01-01T20:00:00Z',
			favourite: false,
			rewatch: true,
			source: 'Letterboxd diary.csv'
		}
	]);

	const [planned] = await rows(
		'letterboxd',
		'watchlist.csv',
		'Date,Name,Year,Letterboxd URI\n2024-03-01,Alien,1979,https://boxd.it/2\n'
	);
	assert.equal(planned.status, 'planned');
	assert.equal(planned.watched_at, null);
	assert.equal(planned.rating, null);

	const [liked] = await rows(
		'letterboxd',
		'likes/films.csv',
		'Date,Name,Year,Letterboxd URI\n2024-03-01,Alien,1979,https://boxd.it/2\n'
	);
	assert.equal(liked.favourite, true);
	assert.equal(liked.status, 'completed');
});

test('IMDb: ratings out of ten, the type column picks film or show', async () => {
	const list = await rows(
		'imdb',
		'ratings.csv',
		'Const,Your Rating,Date Rated,Title,Title Type,Year\n' +
			'tt0113277,8,2023-05-06,Heat,Movie,1995\n' +
			'tt0903747,10,2023-05-07,Breaking Bad,TV Series,2008\n' +
			'tt0078748,,,Alien,Movie,1979\n'
	);
	assert.deepEqual(
		list.map((r) => [r.title, r.kind, r.imdb_id, r.year, r.rating, r.status, r.watched_at]),
		[
			['Heat', 'movie', 'tt0113277', 1995, 80, 'completed', '2023-05-06T20:00:00Z'],
			['Breaking Bad', 'tv', 'tt0903747', 2008, 100, 'completed', '2023-05-07T20:00:00Z'],
			['Alien', 'movie', 'tt0078748', 1979, null, 'planned', null]
		]
	);
});

// Any ISO timestamp keeps its day and is set to 20:00 UTC, the same as a
// Letterboxd date: the time of day a tracker records is not kept.
test('Trakt: ids carried across, ratings out of ten, unwatched is planned', async () => {
	const list = await rows(
		'trakt',
		'history.json',
		JSON.stringify([
			{
				watched_at: '2024-02-03T21:00:00.000Z',
				movie: { title: 'Heat', year: 1995, ids: { tmdb: 949, imdb: 'tt0113277' } }
			},
			{
				rating: 9,
				rated_at: '2024-02-04T10:00:00.000Z',
				show: { title: 'Dark', year: 2017, ids: { tmdb: 70523 } }
			},
			{ show: { title: 'Severance', year: 2022, ids: {} } }
		])
	);
	assert.deepEqual(
		list.map((r) => [r.title, r.kind, r.tmdb_id, r.imdb_id, r.rating, r.status, r.watched_at]),
		[
			['Heat', 'movie', 949, 'tt0113277', null, 'completed', '2024-02-03T20:00:00Z'],
			['Dark', 'tv', 70523, null, 90, 'completed', '2024-02-04T20:00:00Z'],
			['Severance', 'tv', null, null, null, 'planned', null]
		]
	);
	await assert.rejects(rows('trakt', 'empty.json', '[]'), /No entries/);
	await assert.rejects(rows('trakt', 'bad.json', '{'), /not valid JSON/);
});

test('TV Time: one row per show, however many episodes the file lists', async () => {
	const list = await rows(
		'tvtime',
		'seen_episode.csv',
		'show_name,episode_number,created_at\nDark,1,2020-01-01 10:00:00\nDark,2,2020-01-02 10:00:00\nLost,1,2019-05-05 10:00:00\n'
	);
	assert.deepEqual(
		list.map((r) => [r.title, r.kind, r.status, r.source]),
		[
			['Dark', 'tv', 'watching', 'TV Time seen_episode.csv'],
			['Lost', 'tv', 'watching', 'TV Time seen_episode.csv']
		]
	);
	const [film] = await rows('tvtime', 'movies.csv', 'movie name,year\nHeat,1995\n');
	assert.equal(film.kind, 'movie');
	assert.equal(film.year, 1995);
});

test("Frog's own backup comes back with its ids, rating, favourite and first watch", async () => {
	const backup = {
		format: 'frog-library',
		version: 1,
		titles: [
			{
				kind: 'movie',
				tmdb_id: 949,
				name: 'Heat',
				release_date: '1995-12-15',
				status: 'completed',
				rating: 85,
				favourite: true,
				watches: [{ watched_at: '2024-01-01T20:00:00Z', rewatch: false, source: 'manual' }]
			},
			{ kind: 'tv', tmdb_id: 1399, name: 'Game of Thrones', status: 'dropped', rating: null }
		]
	};
	const list = await rows('frog', 'frog-library.json', JSON.stringify(backup));
	assert.deepEqual(
		list.map((r) => [
			r.title,
			r.kind,
			r.tmdb_id,
			r.year,
			r.rating,
			r.status,
			r.favourite,
			r.watched_at
		]),
		[
			['Heat', 'movie', 949, 1995, 85, 'completed', true, '2024-01-01T20:00:00Z'],
			['Game of Thrones', 'tv', 1399, null, null, 'dropped', false, null]
		]
	);
	await assert.rejects(rows('frog', 'other.json', '{"format":"other"}'), /not a Frog backup/);
});

test('a backup names titles by provider ids, never by the Frog id it carries', async () => {
	const v2 = {
		format: 'frog-library',
		version: 2,
		titles: [
			{
				id: 7,
				ids: { tmdb: { kind: 'movie', id: 949 }, anilist: [], imdb: ['tt0113277'] },
				kind: 'movie',
				tmdb_id: 949,
				name: 'Heat'
			},
			{
				id: 8,
				ids: { tmdb: null, anilist: [21], imdb: [] },
				kind: 'tv',
				tmdb_id: null,
				name: 'Only On AniList'
			}
		]
	};
	const v1 = {
		format: 'frog-library',
		version: 1,
		titles: [{ kind: 'tv', tmdb_id: -21, name: 'Only On AniList' }]
	};
	const ids = (list) => list.map((r) => [r.tmdb_id, r.anilist_id, r.imdb_id, r.id ?? null]);
	assert.deepEqual(ids(await rows('frog', 'v2.json', JSON.stringify(v2))), [
		[949, null, 'tt0113277', null],
		[null, 21, null, null]
	]);
	assert.deepEqual(ids(await rows('frog', 'v1.json', JSON.stringify(v1))), [
		[null, 21, null, null]
	]);
});

test('anime titles split into the show and the season they mean', () => {
	const cases = [
		[
			'Attack on Titan Season 3 Part 2',
			{ base: 'Attack on Titan', season: 3, part: 2, unnumbered: false }
		],
		['Mob Psycho 100 II', { base: 'Mob Psycho 100', season: 2, part: null, unnumbered: false }],
		['Overlord IV', { base: 'Overlord', season: 4, part: null, unnumbered: false }],
		[
			'Bungo Stray Dogs 2nd Season',
			{ base: 'Bungo Stray Dogs', season: 2, part: null, unnumbered: false }
		],
		['Haikyu!! 2', { base: 'Haikyu!!', season: 2, part: null, unnumbered: false }],
		[
			'Shingeki no Kyojin: The Final Season',
			{ base: 'Shingeki no Kyojin', season: null, part: null, unnumbered: true }
		],
		[
			'Re:ZERO -Starting Life in Another World- Season 2',
			{ base: 'Re:ZERO -Starting Life in Another World-', season: 2, part: null, unnumbered: false }
		],
		['Cowboy Bebop', { base: 'Cowboy Bebop', season: null, part: null, unnumbered: false }]
	];
	for (const [title, want] of cases) assert.deepEqual(animeTitle(title), want, title);
});

test('the same title from several files becomes one row with the best of each', () => {
	const row = (over) => ({
		key: 'k',
		title: 'Heat',
		year: 1995,
		imdb_id: null,
		tmdb_id: null,
		kind: 'movie',
		rating: null,
		status: 'planned',
		watched_at: null,
		favourite: false,
		rewatch: false,
		source: 'x',
		...over
	});
	const merged = dedupe([
		row({ watched_at: '2024-05-01T20:00:00Z', status: 'completed' }),
		row({ rating: 90, watched_at: '2023-01-01T20:00:00Z' }),
		row({ favourite: true }),
		row({ title: 'Heat', year: 2025 })
	]);
	assert.equal(merged.length, 2, 'a different year is a different film');
	assert.deepEqual(
		[merged[0].rating, merged[0].favourite, merged[0].status, merged[0].watched_at],
		[90, true, 'completed', '2023-01-01T20:00:00Z']
	);
});
