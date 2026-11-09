// Stats as a dashboard: what a click on a bar keeps. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { bucketOf, toggle, view } from '../src/lib/stats-filter.ts';

const fact = (id, fields) => ({
	id,
	kind: 'movie',
	name: `Title ${id}`,
	poster_path: null,
	genres: [],
	year: 2010,
	rating: null,
	units: [],
	...fields
});

const facts = [
	fact(1, { genres: ['Drama'], year: 1999, rating: 90, units: [['2026-01', 120, 1, 0, 0]] }),
	fact(2, {
		kind: 'tv',
		genres: ['Drama', 'Comedy'],
		year: 2015,
		rating: 70,
		units: [
			['2026-01', 60, 0, 2, 0],
			['2026-02', 90, 0, 3, 1]
		]
	}),
	fact(3, { genres: ['Comedy'], year: 2016, rating: null, units: [['2026-02', 100, 1, 0, 0]] }),
	// Rated, never watched in this period.
	fact(4, { genres: ['Horror'], year: 1980, rating: 40, units: [] })
];
const units = ['2026-01', '2026-02', '2026-03'];

test('nothing picked adds up everything', () => {
	const v = view(facts, {}, units);
	assert.equal(v.totals.titles, 3);
	assert.equal(v.totals.minutes, 370);
	assert.equal(v.totals.films, 2);
	assert.equal(v.totals.episodes, 5);
	assert.equal(v.totals.shows, 1);
	assert.equal(v.totals.rated, 3, 'a title only rated still counts towards ratings');
	assert.deepEqual(
		v.timeline.map((t) => t.minutes),
		[180, 190, 0]
	);
});

test('a genre narrows every other chart, and keeps its own bars', () => {
	const v = view(facts, { genre: 'Comedy' }, units);
	assert.equal(v.totals.titles, 2);
	assert.deepEqual(
		v.decades.map((d) => d.decade),
		[2010]
	);
	assert.deepEqual(
		v.genres.map((g) => g.name).sort(),
		['Comedy', 'Drama'],
		'the genre chart is drawn without its own selection'
	);
});

test('a month counts only what was watched in it', () => {
	const v = view(facts, { unit: '2026-02' }, units);
	assert.equal(v.totals.minutes, 190);
	assert.equal(v.totals.episodes, 3);
	assert.equal(v.top_shows[0].watches, 3);
	assert.equal(v.timeline[0].minutes, 180, 'the timeline keeps its other months');
});

test('a ratings bar and a decade combine', () => {
	assert.equal(bucketOf(85), 8);
	assert.equal(bucketOf(3), 0);
	const v = view(facts, { rating: bucketOf(70), decade: 2010 }, units);
	assert.equal(v.totals.titles, 1);
	assert.equal(v.top_shows[0].id, 2);
});

test('picking a bar twice puts it back', () => {
	const once = toggle({}, 'genre', 'Drama');
	assert.deepEqual(once, { genre: 'Drama' });
	assert.deepEqual(toggle(once, 'genre', 'Drama'), {});
	assert.deepEqual(toggle(once, 'genre', 'Comedy'), { genre: 'Comedy' });
});
