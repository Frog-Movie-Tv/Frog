// Shelves as a bookcase: spine widths, what stands face out, the orders.
// Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
	doneBy,
	sortItems,
	spineHeight,
	spineWidth,
	stand,
	THICK,
	THIN
} from '../src/lib/books.ts';

test('a spine is as thick as the title is long, within bounds', () => {
	const short = spineWidth(45);
	const film = spineWidth(100);
	const long = spineWidth(170);
	const season = spineWidth(12 * 24);
	const series = spineWidth(49 * 44);
	const onePiece = spineWidth(1180 * 24);
	assert.equal(short, THIN);
	assert.ok(film > short && long > film, `${short} < ${film} < ${long}`);
	assert.ok(season > long, 'a season of anime is a thicker book than a long film');
	assert.ok(series > season);
	assert.equal(onePiece, THICK);
	assert.equal(spineWidth(0), 20, 'no known length stands as a film');
});

test('a spine height is fixed per title and stays below a poster', () => {
	assert.equal(spineHeight(7), spineHeight(7));
	for (let id = 1; id < 60; id++) {
		const h = spineHeight(id);
		assert.ok(h >= 0.8 && h < 1, String(h));
	}
});

test('a shelf stands some face out, spines after, and counts the rest', () => {
	const books = Array.from({ length: 40 }, (_, i) => ({ id: i, minutes: 100 }));
	const s = stand(books, 52, 1200, 112, 12);
	assert.equal(s.faces.length, 5);
	assert.ok(s.spines.length > 10);
	assert.equal(s.faces.length + s.spines.length + s.rest, 52);
	// A narrow shelf still shows three faces.
	assert.equal(stand(books, 40, 200, 112, 12).faces.length, 3);
	// A short shelf has no spines and nothing left over.
	const few = stand(books.slice(0, 2), 2, 1200, 112, 12);
	assert.deepEqual([few.faces.length, few.spines.length, few.rest], [2, 0, 0]);
});

const items = [
	{ id: 1, name: 'Heat', release_date: '1995-12-15' },
	{ id: 2, name: 'Clayface', release_date: '2026-09-11' },
	{ id: 3, name: 'Undated', release_date: null },
	{ id: 4, name: 'Inception', release_date: '2010-07-16' }
];
const lengths = {
	1: { left: 170, watched: false },
	2: { left: 109, watched: false },
	3: { left: 60, watched: false },
	4: { left: 0, watched: true }
};

test('a shelf sorts by year either way, undated last', () => {
	const ids = (list) => list.map((i) => i.id);
	assert.deepEqual(ids(sortItems(items, 'year', lengths)), [2, 4, 1, 3]);
	assert.deepEqual(ids(sortItems(items, 'year', lengths, true)), [1, 4, 2, 3]);
	assert.deepEqual(ids(sortItems(items, 'order', lengths)), [1, 2, 3, 4]);
});

test('shortest and longest go by time left, watched last', () => {
	const ids = (list) => list.map((i) => i.id);
	assert.deepEqual(ids(sortItems(items, 'short', lengths)), [3, 2, 1, 4]);
	assert.deepEqual(ids(sortItems(items, 'long', lengths)), [1, 2, 3, 4]);
});

test('a watchlist is done after its hours, a day at a time', () => {
	const today = new Date(2026, 10, 23);
	assert.equal(doneBy(4935, 2, today).toDateString(), new Date(2027, 0, 4).toDateString());
	assert.equal(doneBy(60, 2, today).toDateString(), new Date(2026, 10, 24).toDateString());
});
