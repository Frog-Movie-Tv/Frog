// Telling a moving picture from a still one. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { isAnimated, outputType } from '../src/lib/picture.ts';

const ascii = (text) => [...text].map((c) => c.charCodeAt(0));
const big32 = (n) => [(n >>> 24) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255];
const little32 = (n) => big32(n).reverse();

const SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
const pngChunk = (type, body = []) => [...big32(body.length), ...ascii(type), ...body, 0, 0, 0, 0];
const png = (...chunks) => Uint8Array.from([...SIGNATURE, ...chunks.flat()]);

const webpChunk = (type, body = []) => [
	...ascii(type),
	...little32(body.length),
	...body,
	...(body.length % 2 ? [0] : [])
];
function webp(...chunks) {
	const body = [...ascii('WEBP'), ...chunks.flat()];
	return Uint8Array.from([...ascii('RIFF'), ...little32(body.length), ...body]);
}

test('a still PNG is still', () => {
	assert.equal(isAnimated(png(pngChunk('IHDR', Array(13).fill(0)), pngChunk('IDAT', [1]))), false);
});

test('an APNG moves', () => {
	const moving = png(
		pngChunk('IHDR', Array(13).fill(0)),
		pngChunk('acTL', Array(8).fill(0)),
		pngChunk('IDAT', [1])
	);
	assert.equal(isAnimated(moving), true);
});

test('a chunk past the picture data does not count', () => {
	const late = png(pngChunk('IDAT', [1]), pngChunk('acTL', Array(8).fill(0)));
	assert.equal(isAnimated(late), false, 'a PNG can only announce it before the data');
});

test('a plain WebP is still', () => {
	assert.equal(isAnimated(webp(webpChunk('VP8 ', [1, 2, 3]))), false);
	assert.equal(isAnimated(webp(webpChunk('VP8L', [1, 2, 3, 4]))), false);
});

test('a WebP says whether it moves in its header', () => {
	const flags = (byte) => webp(webpChunk('VP8X', [byte, ...Array(9).fill(0)]));
	assert.equal(isAnimated(flags(0x02)), true);
	assert.equal(isAnimated(flags(0x10)), false, 'the transparency flag alone');
});

test('an animation chunk is found past an odd-sized one', () => {
	const moving = webp(webpChunk('ICCP', [1, 2, 3]), webpChunk('ANIM', Array(6).fill(0)));
	assert.equal(isAnimated(moving), true);
});

test('anything else, or too little to read, is still', () => {
	assert.equal(isAnimated(Uint8Array.from([0xff, 0xd8, 0xff, 0xe0])), false);
	assert.equal(isAnimated(new Uint8Array()), false);
	assert.equal(isAnimated(Uint8Array.from(SIGNATURE)), false);
	const cut = png(pngChunk('IHDR', Array(13).fill(0))).subarray(0, 14);
	assert.equal(isAnimated(cut), false);
});

test('only a JPEG stays a JPEG', () => {
	assert.equal(outputType('image/jpeg'), 'image/jpeg');
	assert.equal(outputType('image/png'), 'image/webp');
	assert.equal(outputType('image/heic'), 'image/webp');
});
