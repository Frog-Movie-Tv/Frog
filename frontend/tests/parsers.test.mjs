// The readers for other trackers' exports, each against a small file in that
// tracker's own shape. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { gzipSync, strToU8, zipSync } from 'fflate';
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

// ---------------------------------------------------------------- MyAnimeList

/**
 * Node has no DOMParser, so this stands in for the browser's on the flat shape
 * of a MyAnimeList export. `npm test` checks what happens to the values;
 * reading real XML is the browser's job.
 */
class MalParser {
	parseFromString(text) {
		const tag = /<(\w+)>(?:<!\[CDATA\[(.*?)\]\]>|([^<]*))<\/\1>/g;
		const element = (body) => ({
			getElementsByTagName(name) {
				for (const m of body.matchAll(tag)) {
					if (m[1] === name) return [{ textContent: m[2] ?? m[3] }];
				}
				return [];
			}
		});
		return {
			querySelector: () => (text.includes('<myanimelist>') ? null : {}),
			getElementsByTagName: () =>
				[...text.matchAll(/<anime>(.*?)<\/anime>/gs)].map((m) => element(m[1]))
		};
	}
}

/** An <anime> element as MyAnimeList writes it. */
const anime = ({
	title,
	type = 'TV',
	episodes = 12,
	watched = 0,
	finish = '0000-00-00',
	score = 0,
	status = 'Watching',
	rewatching = 0
}) => `
	<anime>
		<series_animedb_id>1</series_animedb_id>
		<series_title><![CDATA[${title}]]></series_title>
		<series_type>${type}</series_type>
		<series_episodes>${episodes}</series_episodes>
		<my_watched_episodes>${watched}</my_watched_episodes>
		<my_finish_date>${finish}</my_finish_date>
		<my_score>${score}</my_score>
		<my_status>${status}</my_status>
		<my_rewatching>${rewatching}</my_rewatching>
	</anime>`;

const mal = (...entries) => `<?xml version="1.0" encoding="UTF-8" ?>
<myanimelist>
	<myinfo><user_export_type>1</user_export_type></myinfo>${entries.map(anime).join('')}
</myanimelist>`;

async function malRows(...entries) {
	globalThis.DOMParser = MalParser;
	try {
		return await rows('mal', 'animelist.xml', mal(...entries));
	} finally {
		delete globalThis.DOMParser;
	}
}

test('MyAnimeList: a finished season, in full', async () => {
	const list = await malRows({
		title: 'Bungo Stray Dogs 2nd Season',
		episodes: 12,
		watched: 12,
		finish: '2023-04-05',
		score: 9,
		status: 'Completed',
		rewatching: 1
	});
	assert.deepEqual(list.map(shape), [
		{
			title: 'Bungo Stray Dogs',
			year: null,
			imdb_id: null,
			tmdb_id: null,
			kind: 'tv',
			rating: 90,
			status: 'completed',
			watched_at: null,
			favourite: false,
			rewatch: true,
			source: 'MyAnimeList',
			label: 'Bungo Stray Dogs 2nd Season',
			anime: true,
			episode_total: 12,
			alt_titles: [],
			season: 2,
			progress: null
		}
	]);
});

test('MyAnimeList: status, score and progress in its own words', async () => {
	const list = await malRows(
		{ title: 'Cowboy Bebop', watched: 5, status: 'Watching' },
		{ title: 'Trigun', watched: 3, status: 'On-Hold', score: 7 },
		{ title: 'Monster', watched: 1, status: 'Dropped' },
		{ title: 'Berserk', status: 'Plan to Watch', episodes: 0 }
	);
	assert.deepEqual(
		list.map((r) => [r.title, r.status, r.rating, r.progress, r.episode_total, r.season]),
		[
			['Cowboy Bebop', 'watching', null, 5, 12, 1],
			['Trigun', 'paused', 70, 3, 12, 1],
			['Monster', 'dropped', null, 1, 12, 1],
			['Berserk', 'planned', null, null, null, 1]
		]
	);
});

test('MyAnimeList: a special ticks nothing, even when finished', async () => {
	const [special] = await malRows({
		title: 'Cowboy Bebop',
		type: 'OVA',
		episodes: 3,
		watched: 3,
		status: 'Completed'
	});
	assert.equal(special.season, 0);
	// Zero, not nothing: with no count a finished row ticks the whole season.
	assert.equal(special.progress, 0);
	assert.equal(special.warning, 'Special: matched to the show, episodes not ticked');
});

test('MyAnimeList: "The Final Season" asks which one it is', async () => {
	const [row] = await malRows({ title: 'Shingeki no Kyojin: The Final Season', watched: 4 });
	assert.equal(row.title, 'Shingeki no Kyojin');
	assert.equal(row.season, null);
	assert.equal(row.warning, 'Choose which season this is');
});

test('MyAnimeList: later parts start where the part before ended', async () => {
	const list = await malRows(
		{ title: 'Attack on Titan Season 3 Part 2', episodes: 10 },
		{ title: 'Attack on Titan Season 3 Part 1', episodes: 12 },
		{ title: 'Attack on Titan Season 2', episodes: 12 },
		{ title: 'Vinland Saga', episodes: 24 }
	);
	assert.deepEqual(
		list.map((r) => [r.label, r.season, r.episode_offset ?? null, r.warning ?? null, 'part' in r]),
		[
			['Attack on Titan Season 3 Part 2', 3, 12, null, false],
			['Attack on Titan Season 3 Part 1', 3, 0, null, false],
			['Attack on Titan Season 2', 2, null, null, false],
			['Vinland Saga', 1, null, null, false]
		]
	);
});

test('MyAnimeList: a part of unknown length leaves the next one in doubt', async () => {
	const list = await malRows(
		{ title: 'Long Show Part 1', episodes: 0 },
		{ title: 'Long Show Part 2', episodes: 12 },
		{ title: 'Long Show Part 3', episodes: 12 }
	);
	assert.deepEqual(
		list.map((r) => [r.episode_offset, r.warning ?? null]),
		[
			[0, null],
			[0, 'Could not place this part; check the episodes'],
			[12, 'Could not place this part; check the episodes']
		]
	);
});

test('MyAnimeList: a file that is not one says so', async () => {
	await assert.rejects(malRows(), /No anime found/);
	globalThis.DOMParser = MalParser;
	try {
		await assert.rejects(rows('mal', 'x.xml', '<other/>'), /not a MyAnimeList export/);
	} finally {
		delete globalThis.DOMParser;
	}
});

// ---------------------------------------------------------------- archives

test('a gzipped export is unpacked first', async () => {
	globalThis.DOMParser = MalParser;
	try {
		const bytes = gzipSync(strToU8(mal({ title: 'Cowboy Bebop' })));
		const files = await readFile('mal', new File([bytes], 'animelist.xml.gz'));
		assert.deepEqual(
			files.map((f) => [f.name, f.rows.map((r) => r.title)]),
			[['animelist.xml.gz', ['Cowboy Bebop']]]
		);
	} finally {
		delete globalThis.DOMParser;
	}
});

test('a Letterboxd ZIP reads its library and leaves its comments and lists', async () => {
	const header = 'Date,Name,Year,Letterboxd URI\n';
	const zip = zipSync({
		'watched.csv': strToU8(`${header}2024-01-01,Heat,1995,https://boxd.it/1\n`),
		'ratings.csv': strToU8('Date,Name,Year,Letterboxd URI,Rating\n2024-01-02,Alien,1979,x,3.5\n'),
		'likes/films.csv': strToU8(`${header}2024-01-03,Heat,1995,https://boxd.it/1\n`),
		'comments.csv': strToU8(`${header}2024-01-04,Not a film,2000,x\n`),
		'reviews.csv': strToU8(`${header}2024-01-05,Not a film,2000,x\n`),
		'lists/best.csv': strToU8(`${header}2024-01-06,Not a film,2000,x\n`),
		'profile.txt': strToU8('hello')
	});
	const files = await readFile('letterboxd', new File([zip], 'letterboxd-export.zip'));
	assert.deepEqual(
		files.map((f) => [f.name, f.rows.map((r) => [r.title, r.status, r.rating, r.favourite])]),
		[
			['watched.csv', [['Heat', 'completed', null, false]]],
			['ratings.csv', [['Alien', 'completed', 70, false]]],
			['likes/films.csv', [['Heat', 'completed', null, true]]]
		]
	);
	const empty = zipSync({ 'readme.txt': strToU8('hello') });
	await assert.rejects(readFile('letterboxd', new File([empty], 'x.zip')), /No usable CSV files/);
});

// ---------------------------------------------------------------- corners

test('Letterboxd: half stars, blank rows and a diary with no watched date', async () => {
	const list = await rows(
		'letterboxd',
		'diary.csv',
		'Date,Name,Year,Letterboxd URI,Rating,Rewatch,Tags,Watched Date\n' +
			'2024-01-02,Heat,1995,x,0.5,,,\n' +
			',,,,,,,\n' +
			'2024-01-03,Alien,,x,,No,,2024-01-01\n'
	);
	assert.deepEqual(
		list.map((r) => [r.title, r.year, r.rating, r.rewatch, r.watched_at]),
		[
			['Heat', 1995, 10, false, '2024-01-02T20:00:00Z'],
			['Alien', null, null, false, '2024-01-01T20:00:00Z']
		]
	);
});

test('IMDb: every word it has for a show, and a row with no title is skipped', async () => {
	const list = await rows(
		'imdb',
		'watchlist.csv',
		'Const,Your Rating,Date Rated,Title,Original Title,Title Type,Year\n' +
			'tt1,,,Mini,,TV Mini Series,2019\n' +
			'tt2,,,Old,,tvSeries,2001\n' +
			'tt3,,,Short,,Short,2010\n' +
			'tt4,,,,Originale,Movie,2011\n' +
			'tt5,,,,,Movie,2012\n'
	);
	assert.deepEqual(
		list.map((r) => [r.title, r.kind, r.imdb_id, r.status]),
		[
			['Mini', 'tv', 'tt1', 'planned'],
			['Old', 'tv', 'tt2', 'planned'],
			['Short', 'movie', 'tt3', 'planned'],
			['Originale', 'movie', 'tt4', 'planned']
		]
	);
});

test('Trakt: a list inside "items", and a bare title with no kind', async () => {
	const list = await rows(
		'trakt',
		'list.json',
		JSON.stringify({
			items: [{ title: 'Heat', year: 1995 }, { rating: 0, movie: { title: 'Alien' } }, {}]
		})
	);
	assert.deepEqual(
		list.map((r) => [r.title, r.kind, r.year, r.rating, r.status]),
		[
			['Heat', null, 1995, null, 'planned'],
			['Alien', 'movie', null, null, 'planned']
		]
	);
});

test("Trakt: one episode is not the whole show, and its rating is not the show's", async () => {
	const list = await rows(
		'trakt',
		'history.json',
		JSON.stringify([
			{
				type: 'episode',
				watched_at: '2024-02-03T21:00:00.000Z',
				episode: { season: 1, number: 1, title: 'Pilot' },
				show: { title: 'Dark', year: 2017, ids: { tmdb: 70523 } }
			},
			{
				type: 'episode',
				rating: 3,
				rated_at: '2024-02-04T10:00:00.000Z',
				episode: { season: 1, number: 2 },
				show: { title: 'Lost', ids: { tmdb: 4607 } }
			},
			{
				type: 'season',
				rating: 4,
				rated_at: '2024-02-05T10:00:00.000Z',
				season: { number: 1 },
				show: { title: 'Severance', ids: { tmdb: 95396 } }
			}
		])
	);
	assert.deepEqual(
		list.map((r) => [r.title, r.rating, r.status, r.watched_at]),
		[
			['Dark', null, 'watching', '2024-02-03T20:00:00Z'],
			['Lost', null, 'watching', '2024-02-04T20:00:00Z'],
			['Severance', null, 'watching', '2024-02-05T20:00:00Z']
		]
	);
});

test('TV Time: its other column names, the first date, and an empty file', async () => {
	const list = await rows(
		'tvtime',
		'tracking-prod-records.csv',
		'tv_show_name,first_aired,created_at\nDark,2017-12-01,2020-01-01 10:00:00\nDark,2017-12-01,2021-01-01 10:00:00\n,,\n'
	);
	assert.deepEqual(
		list.map((r) => [r.title, r.kind, r.year, r.watched_at]),
		[['Dark', 'tv', 2017, '2020-01-01T20:00:00Z']]
	);
	assert.deepEqual(await rows('tvtime', 'empty.csv', 'show_name\n'), []);
});

test('a Frog backup brings back the episodes that were watched', async () => {
	const backup = {
		format: 'frog-library',
		version: 2,
		titles: [
			{
				kind: 'tv',
				tmdb_id: 1399,
				name: 'Game of Thrones',
				status: 'watching',
				episodes: [
					{ season: 1, episode: 1, rating: null, notes: null, watched: true },
					{ season: 1, episode: 2, rating: null, notes: 'a note', watched: false },
					{ season: 2, episode: 1, rating: null, notes: null, watched: true }
				]
			},
			{ kind: 'tv', tmdb_id: 70523, name: 'Dark', status: 'planned', episodes: [] }
		]
	};
	const [thrones, dark] = await rows('frog', 'frog-library.json', JSON.stringify(backup));
	assert.deepEqual(thrones.watches, [
		{ season: 1, episode: 1, watched_at: null },
		{ season: 2, episode: 1, watched_at: null }
	]);
	assert.equal(thrones.progress, 2);
	assert.equal(dark.watches, undefined);
	assert.equal(dark.progress, undefined);
});
