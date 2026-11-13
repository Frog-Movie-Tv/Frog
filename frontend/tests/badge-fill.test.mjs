// The badge editor's background fill. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { borderColour, coverage, decontaminate, removeBackground } from '../src/lib/badge-fill.ts';

/** A square picture from rows of single letters: w white, k black, r red. */
function picture(rows) {
	const colours = { w: [255, 255, 255], k: [0, 0, 0], r: [220, 30, 40], g: [128, 128, 128] };
	const size = rows.length;
	const data = new Uint8ClampedArray(size * size * 4);
	rows.forEach((row, y) =>
		[...row].forEach((c, x) => {
			const i = (y * size + x) * 4;
			data.set([...colours[c], 255], i);
		})
	);
	return { data, size };
}

const alpha = (data, size, x, y) => data[(y * size + x) * 4 + 3];

test('the border colour is the most common one round the edge', () => {
	const { data, size } = picture(['wwwww', 'wkkkw', 'wkrkw', 'wkkkw', 'wwwkk']);
	assert.deepEqual(borderColour(data, size, size), [255, 255, 255]);
});

test('only what the border reaches is cleared, so white inside a letter stays', () => {
	// A black ring with white inside it, on white.
	const { data, size } = picture(['wwwww', 'wkkkw', 'wkwkw', 'wkkkw', 'wwwww']);
	removeBackground(data, size, size, { tolerance: 10, softness: 0 });
	assert.equal(alpha(data, size, 0, 0), 0, 'the background goes');
	assert.equal(alpha(data, size, 1, 1), 255, 'the ring stays');
	assert.equal(alpha(data, size, 2, 2), 255, 'the white inside stays');
});

test('a tapped spot clears a region the border could not reach', () => {
	const { data, size } = picture(['wwwww', 'wkkkw', 'wkwkw', 'wkkkw', 'wwwww']);
	removeBackground(data, size, size, { tolerance: 10, softness: 0, spots: [2 * size + 2] });
	assert.equal(alpha(data, size, 2, 2), 0);
	assert.equal(alpha(data, size, 1, 1), 255);
});

test('the edge band fades, and loses the background from its colour', () => {
	// Grey sits halfway between white and black: inside a wide band, half solid.
	const { data, size } = picture(['wwwww', 'wgggw', 'wgkgw', 'wgggw', 'wwwww']);
	removeBackground(data, size, size, { tolerance: 5, softness: 90 });
	const a = alpha(data, size, 1, 1);
	assert.ok(a > 60 && a < 200, `half solid, got ${a}`);
	// What is left of it is darker than the grey seen over white.
	assert.ok(data[(1 * size + 1) * 4] < 128);
	assert.equal(alpha(data, size, 2, 2), 255, 'black is past the band');
});

test('coverage and decontamination on plain numbers', () => {
	assert.equal(coverage(5, 10, 10), 0);
	assert.equal(coverage(15, 10, 10), 0.5);
	assert.equal(coverage(25, 10, 10), 1);
	assert.equal(coverage(11, 10, 0), 1, 'no softness is a hard cut');
	// Half-covered black over white looks 128; taken out, it is black again.
	assert.equal(decontaminate(128, 255, 0.5), 1);
	assert.equal(decontaminate(200, 200, 1), 200);
});

test('a wide band softens the edge but never eats a pale region of the picture', () => {
	// A pale grey block, far wider than an edge, beside the white background.
	const rows = [];
	for (let y = 0; y < 12; y++) rows.push(y === 0 || y === 11 ? 'wwwwwwwwwwww' : 'wggggggggggw');
	const { data, size } = picture(rows);
	removeBackground(data, size, size, { tolerance: 5, softness: 95 });
	assert.ok(alpha(data, size, 1, 1) < 255, 'the outer ring of it softens');
	assert.equal(alpha(data, size, 6, 6), 255, 'its middle stays solid');
});
