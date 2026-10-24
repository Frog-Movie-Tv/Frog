/**
 * Generates the PWA icons from code, so the repo carries no binary blobs that
 * nobody can regenerate. Run with `npm run icons` after changing the brand.
 *
 * Writes a minimal PNG by hand (IHDR + IDAT + IEND) rather than pulling in an
 * image library for two files.
 */
import { deflateSync } from 'node:zlib';
import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const staticDir = join(here, '..', 'static');

const CRC_TABLE = (() => {
	const table = new Int32Array(256);
	for (let n = 0; n < 256; n++) {
		let c = n;
		for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
		table[n] = c;
	}
	return table;
})();

function crc32(buf) {
	let c = -1;
	for (const byte of buf) c = CRC_TABLE[(c ^ byte) & 0xff] ^ (c >>> 8);
	return (c ^ -1) >>> 0;
}

function chunk(type, data) {
	const head = Buffer.alloc(8);
	head.writeUInt32BE(data.length, 0);
	head.write(type, 4, 'ascii');
	const crc = Buffer.alloc(4);
	crc.writeUInt32BE(crc32(Buffer.concat([head.subarray(4), data])), 0);
	return Buffer.concat([head, data, crc]);
}

function png(width, height, rgba) {
	const ihdr = Buffer.alloc(13);
	ihdr.writeUInt32BE(width, 0);
	ihdr.writeUInt32BE(height, 4);
	ihdr[8] = 8; // bit depth
	ihdr[9] = 6; // truecolour with alpha
	// Each scanline is prefixed with filter type 0 (none).
	const stride = width * 4;
	const raw = Buffer.alloc((stride + 1) * height);
	for (let y = 0; y < height; y++) {
		raw[y * (stride + 1)] = 0;
		rgba.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
	}
	return Buffer.concat([
		Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
		chunk('IHDR', ihdr),
		chunk('IDAT', deflateSync(raw, { level: 9 })),
		chunk('IEND', Buffer.alloc(0))
	]);
}

const mix = (a, b, t) => a.map((v, i) => Math.round(v + (b[i] - v) * t));

/**
 * A rounded square carrying the brand gradient, with a play triangle knocked
 * out of it. `maskable` fills the whole canvas so Android can crop it to any
 * shape without clipping the mark.
 */
function render(size, maskable) {
	const data = Buffer.alloc(size * size * 4);
	const from = [122, 92, 255];
	const to = [40, 225, 160];

	const pad = maskable ? 0 : size * 0.09;
	const inner = size - pad * 2;
	const radius = maskable ? 0 : inner * 0.235;

	for (let y = 0; y < size; y++) {
		for (let x = 0; x < size; x++) {
			const i = (y * size + x) * 4;
			const lx = x - pad;
			const ly = y - pad;

			let alpha = 0;
			if (lx >= 0 && ly >= 0 && lx < inner && ly < inner) {
				alpha = radius > 0 ? roundedAlpha(lx, ly, inner, radius) : 1;
			}
			if (alpha <= 0) continue;

			const [r, g, b] = mix(from, to, (lx / inner) * 0.55 + (ly / inner) * 0.45);
			const inTriangle = triangle(lx, ly, inner);

			data[i] = inTriangle ? 255 : r;
			data[i + 1] = inTriangle ? 255 : g;
			data[i + 2] = inTriangle ? 255 : b;
			data[i + 3] = Math.round(alpha * 255);
		}
	}
	return png(size, size, data);
}

/** Antialiased rounded-rectangle coverage for one pixel. */
function roundedAlpha(x, y, size, radius) {
	const cx = Math.min(Math.max(x, radius), size - radius);
	const cy = Math.min(Math.max(y, radius), size - radius);
	const d = Math.hypot(x - cx, y - cy);
	if (d <= radius - 1) return 1;
	if (d >= radius) return 0;
	return radius - d;
}

/** The play mark: a triangle centred in the tile. */
function triangle(x, y, size) {
	const left = size * 0.38;
	const right = size * 0.66;
	const top = size * 0.3;
	const bottom = size * 0.7;
	if (x < left || x > right) return false;
	const t = (x - left) / (right - left);
	const half = ((bottom - top) / 2) * (1 - t);
	const mid = (top + bottom) / 2;
	return y > mid - half && y < mid + half;
}

mkdirSync(staticDir, { recursive: true });

writeFileSync(join(staticDir, 'icon-192.png'), render(192, false));
writeFileSync(join(staticDir, 'icon-512.png'), render(512, false));
writeFileSync(join(staticDir, 'icon-maskable-512.png'), render(512, true));

const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
  <defs>
    <linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#7a5cff"/>
      <stop offset="1" stop-color="#28e1a0"/>
    </linearGradient>
  </defs>
  <rect x="3" y="3" width="58" height="58" rx="15" fill="url(#g)"/>
  <path d="M26 20.5 44 32 26 43.5z" fill="#fff"/>
</svg>
`;
writeFileSync(join(staticDir, 'icon.svg'), svg);

console.log('icons written to static/');
