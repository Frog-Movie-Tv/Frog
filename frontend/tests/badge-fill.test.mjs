// The badge editor's background fill. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
	borderColour,
	decontaminate,
	difference,
	removeBackground,
	suggestTolerance,
	unmix
} from '../src/lib/badge-fill.ts';

/** A square picture from rows of single letters: w white, k black, r red, p pale pink. */
function picture(rows) {
	const colours = {
		w: [255, 255, 255],
		k: [0, 0, 0],
		r: [220, 30, 40],
		g: [128, 128, 128],
		p: [255, 214, 232]
	};
	const size = rows.length;
	const data = new Uint8ClampedArray(size * size * 4);
	rows.forEach((row, y) =>
		[...row].forEach((c, x) => data.set([...colours[c], 255], (y * size + x) * 4))
	);
	return { data, size };
}

/** A disc of `colour` on white, its edge smoothed as a drawing program would. */
function disc(size, colour, radius) {
	const data = new Uint8ClampedArray(size * size * 4);
	const c = size / 2;
	for (let y = 0; y < size; y++)
		for (let x = 0; x < size; x++) {
			// Four by four samples a pixel: how much of it the disc covers.
			let inside = 0;
			for (let sy = 0; sy < 4; sy++)
				for (let sx = 0; sx < 4; sx++)
					if (Math.hypot(x + (sx + 0.5) / 4 - c, y + (sy + 0.5) / 4 - c) < radius) inside++;
			const a = inside / 16;
			const i = (y * size + x) * 4;
			for (let k = 0; k < 3; k++) data[i + k] = Math.round(colour[k] * a + 255 * (1 - a));
			data[i + 3] = 255;
		}
	return data;
}

const alpha = (data, size, x, y) => data[(y * size + x) * 4 + 3];

test('the border colour is the most common one round the edge', () => {
	const { data, size } = picture(['wwwww', 'wkkkw', 'wkrkw', 'wkkkw', 'wwwkk']);
	assert.deepEqual(borderColour(data, size, size), [255, 255, 255]);
});

test('only what the border reaches is cleared, so white inside a letter stays', () => {
	const { data, size } = picture([
		'wwwwwww',
		'wkkkkkw',
		'wkwwwkw',
		'wkwwwkw',
		'wkwwwkw',
		'wkkkkkw',
		'wwwwwww'
	]);
	removeBackground(data, size, size, { tolerance: 8 });
	assert.equal(alpha(data, size, 0, 0), 0, 'the background goes');
	assert.equal(alpha(data, size, 3, 3), 255, 'the white inside stays');
	assert.ok(alpha(data, size, 1, 1) > 200, 'the ring stays');
});

test('a tapped spot clears a region the border could not reach', () => {
	const { data, size } = picture([
		'wwwwwww',
		'wkkkkkw',
		'wkwwwkw',
		'wkwwwkw',
		'wkwwwkw',
		'wkkkkkw',
		'wwwwwww'
	]);
	removeBackground(data, size, size, { tolerance: 8, spots: [3 * size + 3] });
	assert.equal(alpha(data, size, 3, 3), 0);
});

test('a pale pastel is not taken for white', () => {
	assert.ok(difference([255, 255, 255], [255, 214, 232]) > 8, 'pale pink reads apart from white');
	const rows = ['wwwwwwwww', ...Array(7).fill('wpppppppw'), 'wwwwwwwww'];
	const { data, size } = picture(rows);
	removeBackground(data, size, size, { tolerance: suggestTolerance(data, size, size) });
	assert.equal(alpha(data, size, 4, 4), 255, 'the pink stays');
	assert.equal(alpha(data, size, 0, 4), 0);
});

test('a smoothed edge keeps its shape and loses every trace of the white', () => {
	const size = 64;
	const red = [200, 20, 60];
	const data = disc(size, red, 24);
	removeBackground(data, size, size, { tolerance: suggestTolerance(data, size, size) });
	let edge = 0;
	for (let p = 0; p < size * size; p++) {
		const a = data[p * 4 + 3];
		if (a === 0) continue;
		const colour = [data[p * 4], data[p * 4 + 1], data[p * 4 + 2]];
		// Whatever stays is the disc's red, at the edge too: no pale fringe.
		assert.ok(difference(colour, red) < 6, `pixel ${p} is ${colour} at ${a}`);
		if (a < 255) edge++;
	}
	assert.ok(edge > 40, `the edge stays smooth (${edge} part-clear pixels)`);
	// And the disc is still its size: its middle and its edge where they were.
	assert.equal(data[(32 * size + 32) * 4 + 3], 255);
	assert.equal(data[(32 * size + 32 + 30) * 4 + 3], 0);
});

test('trim takes the outside off first', () => {
	const size = 64;
	const plain = disc(size, [200, 20, 60], 24);
	const trimmed = plain.slice();
	removeBackground(plain, size, size, { tolerance: 8 });
	removeBackground(trimmed, size, size, { tolerance: 8, trim: 2 });
	const solid = (d) => d.filter((_, i) => i % 4 === 3 && d[i] > 0).length;
	assert.ok(solid(trimmed) < solid(plain));
});

test('unmixing and decontamination on plain numbers', () => {
	assert.equal(unmix([128, 128, 128], [255, 255, 255], [0, 0, 0]).toFixed(2), '0.50');
	assert.equal(unmix([250, 250, 250], [255, 255, 255], [252, 252, 252]), null, 'too alike');
	// Half-covered black over white looks 128; taken out, it is black again.
	assert.equal(decontaminate(128, 255, 0.5), 1);
	assert.equal(decontaminate(200, 200, 1), 200);
});

test('the strength suggested follows the grain of the background', () => {
	const flat = picture(Array(9).fill('wwwwwwwww'));
	const grainy = picture(Array(9).fill('wwwwwwwww'));
	for (let i = 0; i < grainy.data.length; i += 4)
		for (let k = 0; k < 3; k++) grainy.data[i + k] = 255 - ((i * 7 + k * 13) % 30);
	assert.ok(
		suggestTolerance(grainy.data, 9, 9) > suggestTolerance(flat.data, 9, 9),
		'more grain, a stronger start'
	);
});

test('sealing keeps the fill from leaking through a gap in an outline', () => {
	// A black ring with a one-pixel gap at the top, white inside it.
	const rows = [
		'wwwwwwwww',
		'wkkkwkkkw',
		'wkwwwwwkw',
		'wkwwwwwkw',
		'wkwwwwwkw',
		'wkwwwwwkw',
		'wkwwwwwkw',
		'wkkkkkkkw',
		'wwwwwwwww'
	];
	const leaky = picture(rows);
	removeBackground(leaky.data, leaky.size, leaky.size, { tolerance: 8 });
	assert.equal(alpha(leaky.data, leaky.size, 4, 4), 0, 'without it, the inside drains');
	const sealed = picture(rows);
	removeBackground(sealed.data, sealed.size, sealed.size, { tolerance: 8, seal: 1 });
	assert.equal(alpha(sealed.data, sealed.size, 4, 5), 255, 'with it, the inside stays');
	assert.equal(alpha(sealed.data, sealed.size, 0, 4), 0, 'and the outside still goes');
});

test('a smoothed outline turns a staircase into a curve, in its own colour', () => {
	const size = 40;
	const data = new Uint8ClampedArray(size * size * 4);
	// A black triangle with a hard, stepped edge, on white.
	for (let y = 0; y < size; y++)
		for (let x = 0; x < size; x++) {
			const inside = x < y;
			data.set(inside ? [0, 0, 0, 255] : [255, 255, 255, 255], (y * size + x) * 4);
		}
	const hard = data.slice();
	removeBackground(hard, size, size, { tolerance: 8 });
	removeBackground(data, size, size, { tolerance: 8, smooth: 1 });
	const partial = (d) => d.filter((v, i) => i % 4 === 3 && v > 0 && v < 255).length;
	assert.ok(partial(data) > partial(hard), 'more of the edge is part covered');
	for (let p = 0; p < size * size; p++) {
		const a = data[p * 4 + 3];
		if (a > 0) assert.ok(data[p * 4] < 60, `pixel ${p} stays dark, not grey from the white`);
	}
});
