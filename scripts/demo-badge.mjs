/**
 * DEVELOPMENT ONLY. A badge picture for demo data, as the badge editor makes
 * one: a round, lossless PNG, a disc of colour with a light ring and a pale
 * centre, its edge smoothed. Plain zlib, no image library. Used by
 * `scripts/seed-demo.mjs` and `frontend/scripts/screenshots.mjs`.
 */
import { deflateSync } from 'node:zlib';

const CRC = new Uint32Array(256).map((_, n) => {
	let c = n;
	for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
	return c >>> 0;
});

function crc32(bytes) {
	let c = 0xffffffff;
	for (const b of bytes) c = CRC[(c ^ b) & 0xff] ^ (c >>> 8);
	return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
	const out = Buffer.alloc(12 + data.length);
	out.writeUInt32BE(data.length, 0);
	out.write(type, 4, 'ascii');
	data.copy(out, 8);
	out.writeUInt32BE(crc32(out.subarray(4, 8 + data.length)), 8 + data.length);
	return out;
}

/** A `size` px badge in `colour` ([r, g, b]), with `ring` round its edge. */
export function badgePng(size, colour = [79, 187, 88], ring = [226, 247, 228]) {
	const rows = Buffer.alloc(size * (size * 4 + 1));
	const r = size / 2;
	for (let y = 0; y < size; y++) {
		rows[y * (size * 4 + 1)] = 0;
		for (let x = 0; x < size; x++) {
			const d = Math.hypot(x + 0.5 - r, y + 0.5 - r);
			const edge = Math.min(1, Math.max(0, r - d));
			const onRing = d > r * 0.8 && d < r * 0.88;
			const [cr, cg, cb] = onRing ? ring : d < r * 0.3 ? [255, 255, 255] : colour;
			const at = y * (size * 4 + 1) + 1 + x * 4;
			rows[at] = cr;
			rows[at + 1] = cg;
			rows[at + 2] = cb;
			rows[at + 3] = Math.round(edge * 255);
		}
	}
	const head = Buffer.alloc(13);
	head.writeUInt32BE(size, 0);
	head.writeUInt32BE(size, 4);
	head[8] = 8;
	head[9] = 6;
	return Buffer.concat([
		Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
		chunk('IHDR', head),
		chunk('IDAT', deflateSync(rows)),
		chunk('IEND', Buffer.alloc(0))
	]);
}

/** A badge as the badge routes take one, ready to POST. */
export function badgeBody(name, description, colour, ring) {
	const large = badgePng(1024, colour, ring);
	return {
		name,
		description,
		settings: {},
		large: large.toString('base64'),
		small: badgePng(128, colour, ring).toString('base64'),
		original: large.toString('base64')
	};
}
