/**
 * Generates every piece of Frog's identity from the shared mascot geometry:
 * SVG logos, PNG PWA icons and a per-variant web manifest.
 *
 * Run with `npm run icons` after changing anything in mascots.mjs. Nothing is
 * hand-drawn and no binary is committed that cannot be rebuilt.
 *
 * The PNG writer is deliberately dependency-free: a small analytic rasteriser
 * with 4x4 supersampling, then a hand-rolled PNG encoder.
 */
import { deflateSync } from 'node:zlib';
import { mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { POND, VARIANTS, VARIANT_IDS, toSvg } from './mascots.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const staticDir = join(here, '..', 'static');
const mascotDir = join(staticDir, 'mascots');

/** Pond background, matching --bg of the Pond theme. */
const POND_BG = '#07120e';
const GRID = 64;

// ---------------------------------------------------------------- raster ---

const SS = 4; // supersampling factor per axis

function inEllipse(x, y, s) {
	const dx = (x - s.cx) / s.rx;
	const dy = (y - s.cy) / s.ry;
	return dx * dx + dy * dy <= 1;
}

function inCircle(x, y, s) {
	const dx = x - s.cx;
	const dy = y - s.cy;
	return dx * dx + dy * dy <= s.r * s.r;
}

function inRoundedRect(x, y, s) {
	const r = Math.min(s.r ?? 0, s.w / 2, s.h / 2);
	if (x < s.x || x > s.x + s.w || y < s.y || y > s.y + s.h) return false;
	const cx = Math.min(Math.max(x, s.x + r), s.x + s.w - r);
	const cy = Math.min(Math.max(y, s.y + r), s.y + s.h - r);
	const dx = x - cx;
	const dy = y - cy;
	return dx * dx + dy * dy <= r * r;
}

function distanceToSegment(px, py, [x0, y0], [x1, y1]) {
	const dx = x1 - x0;
	const dy = y1 - y0;
	const lenSq = dx * dx + dy * dy;
	const t = lenSq === 0 ? 0 : Math.max(0, Math.min(1, ((px - x0) * dx + (py - y0) * dy) / lenSq));
	const qx = x0 + t * dx;
	const qy = y0 + t * dy;
	return Math.hypot(px - qx, py - qy);
}

function inStroke(x, y, s) {
	const half = s.width / 2;
	for (let i = 0; i < s.points.length - 1; i++) {
		if (distanceToSegment(x, y, s.points[i], s.points[i + 1]) <= half) return true;
	}
	return false;
}

function hits(x, y, shape) {
	switch (shape.type) {
		case 'ellipse':
			return inEllipse(x, y, shape);
		case 'circle':
			return inCircle(x, y, shape);
		case 'rect':
			return inRoundedRect(x, y, shape);
		case 'stroke':
			return inStroke(x, y, shape);
		default:
			return false;
	}
}

function bounds(shape) {
	switch (shape.type) {
		case 'ellipse':
			return [shape.cx - shape.rx, shape.cy - shape.ry, shape.cx + shape.rx, shape.cy + shape.ry];
		case 'circle':
			return [shape.cx - shape.r, shape.cy - shape.r, shape.cx + shape.r, shape.cy + shape.r];
		case 'rect':
			return [shape.x, shape.y, shape.x + shape.w, shape.y + shape.h];
		case 'stroke': {
			const xs = shape.points.map((p) => p[0]);
			const ys = shape.points.map((p) => p[1]);
			const pad = shape.width / 2 + 1;
			return [
				Math.min(...xs) - pad,
				Math.min(...ys) - pad,
				Math.max(...xs) + pad,
				Math.max(...ys) + pad
			];
		}
		default:
			return [0, 0, 0, 0];
	}
}

function parseHex(hex) {
	const v = hex.replace('#', '');
	return [parseInt(v.slice(0, 2), 16), parseInt(v.slice(2, 4), 16), parseInt(v.slice(4, 6), 16)];
}

/**
 * Paint shapes onto an RGBA buffer.
 * `scale` maps the 64-unit grid onto pixels; `offset` shifts it.
 */
function paint(buf, size, shapes, scale, offsetX, offsetY) {
	for (const shape of shapes) {
		const [bx0, by0, bx1, by1] = bounds(shape);
		const px0 = Math.max(0, Math.floor(bx0 * scale + offsetX) - 1);
		const py0 = Math.max(0, Math.floor(by0 * scale + offsetY) - 1);
		const px1 = Math.min(size - 1, Math.ceil(bx1 * scale + offsetX) + 1);
		const py1 = Math.min(size - 1, Math.ceil(by1 * scale + offsetY) + 1);

		const colour = parseHex(shape.fill ?? shape.color ?? '#000000');
		const alpha = shape.opacity ?? 1;

		for (let py = py0; py <= py1; py++) {
			for (let px = px0; px <= px1; px++) {
				let covered = 0;
				for (let sy = 0; sy < SS; sy++) {
					for (let sx = 0; sx < SS; sx++) {
						const gx = (px + (sx + 0.5) / SS - offsetX) / scale;
						const gy = (py + (sy + 0.5) / SS - offsetY) / scale;
						if (hits(gx, gy, shape)) covered++;
					}
				}
				if (!covered) continue;

				const a = (covered / (SS * SS)) * alpha;
				const i = (py * size + px) * 4;
				const dstA = buf[i + 3] / 255;
				const outA = a + dstA * (1 - a);
				for (let ch = 0; ch < 3; ch++) {
					const src = colour[ch];
					const dst = buf[i + ch];
					buf[i + ch] = Math.round((src * a + dst * dstA * (1 - a)) / (outA || 1));
				}
				buf[i + 3] = Math.round(outA * 255);
			}
		}
	}
}

function fillRounded(buf, size, hex, radius) {
	const [r, g, b] = parseHex(hex);
	for (let y = 0; y < size; y++) {
		for (let x = 0; x < size; x++) {
			let covered = 0;
			for (let sy = 0; sy < SS; sy++) {
				for (let sx = 0; sx < SS; sx++) {
					const px = x + (sx + 0.5) / SS;
					const py = y + (sy + 0.5) / SS;
					if (inRoundedRect(px, py, { x: 0, y: 0, w: size, h: size, r: radius })) covered++;
				}
			}
			if (!covered) continue;
			const i = (y * size + x) * 4;
			buf[i] = r;
			buf[i + 1] = g;
			buf[i + 2] = b;
			buf[i + 3] = Math.round((covered / (SS * SS)) * 255);
		}
	}
}

// ------------------------------------------------------------------ png ---

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

function png(size, rgba) {
	const ihdr = Buffer.alloc(13);
	ihdr.writeUInt32BE(size, 0);
	ihdr.writeUInt32BE(size, 4);
	ihdr[8] = 8; // bit depth
	ihdr[9] = 6; // truecolour with alpha

	const stride = size * 4;
	const raw = Buffer.alloc((stride + 1) * size);
	for (let y = 0; y < size; y++) {
		raw[y * (stride + 1)] = 0; // filter: none
		rgba.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
	}

	return Buffer.concat([
		Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
		chunk('IHDR', ihdr),
		chunk('IDAT', deflateSync(raw, { level: 9 })),
		chunk('IEND', Buffer.alloc(0))
	]);
}

/**
 * `maskable` fills the canvas edge to edge and shrinks the frog into Android's
 * safe zone, so any mask shape crops background rather than the face.
 */
function renderIcon(shapes, size, { maskable = false } = {}) {
	const buf = Buffer.alloc(size * size * 4);
	fillRounded(buf, size, POND_BG, maskable ? 0 : size * 0.22);

	const inset = maskable ? 0.58 : 0.84;
	const scale = (size / GRID) * inset;
	const offset = (size - GRID * scale) / 2;
	paint(buf, size, shapes, scale, offset, offset);

	return png(size, buf);
}

/**
 * Android's status-bar badge: white on transparent, nothing else. The frog's
 * dark details (pupils, mouth) become holes so it still reads as a face at
 * 24dp.
 */
function renderBadge(shapes, size) {
	const buf = Buffer.alloc(size * size * 4);
	const scale = (size / GRID) * 0.92;
	const offset = (size - GRID * scale) / 2;
	paint(buf, size, shapes, scale, offset, offset);
	for (let i = 0; i < buf.length; i += 4) {
		const lum = (0.2126 * buf[i] + 0.7152 * buf[i + 1] + 0.0722 * buf[i + 2]) / 255;
		const keep = Math.min(1, Math.max(0, (lum - 0.12) / 0.15));
		buf[i + 3] = Math.round(buf[i + 3] * keep);
		buf[i] = buf[i + 1] = buf[i + 2] = 255;
	}
	return png(size, buf);
}

// ----------------------------------------------------------------- emit ---

rmSync(mascotDir, { recursive: true, force: true });
mkdirSync(mascotDir, { recursive: true });
mkdirSync(staticDir, { recursive: true });

const written = [];
function emit(path, data) {
	writeFileSync(path, data);
	written.push(path.replace(staticDir, 'static'));
}

for (const id of VARIANT_IDS) {
	const shapes = VARIANTS[id].build(POND);

	// Transparent SVG for the in-app logo and the favicon.
	emit(join(mascotDir, `${id}.svg`), toSvg(shapes));
	// Badged SVG, for anywhere that needs its own backdrop.
	emit(join(mascotDir, `${id}-badge.svg`), toSvg(shapes, { background: POND_BG }));

	emit(join(mascotDir, `${id}-192.png`), renderIcon(shapes, 192));
	emit(join(mascotDir, `${id}-512.png`), renderIcon(shapes, 512));
	emit(join(mascotDir, `${id}-maskable-512.png`), renderIcon(shapes, 512, { maskable: true }));

	emit(
		join(staticDir, `manifest-${id}.webmanifest`),
		Buffer.from(JSON.stringify(manifest(id), null, '\t') + '\n')
	);
}

// The defaults the app ships with, so nothing depends on a chosen variant.
const classic = VARIANTS.classic.build(POND);
emit(join(staticDir, 'icon.svg'), toSvg(classic));
emit(join(staticDir, 'icon-192.png'), renderIcon(classic, 192));
emit(join(staticDir, 'icon-512.png'), renderIcon(classic, 512));
emit(join(staticDir, 'icon-maskable-512.png'), renderIcon(classic, 512, { maskable: true }));
emit(join(staticDir, 'icon-mono.svg'), toSvg(classic, { monochrome: 'currentColor' }));
emit(join(staticDir, 'badge-96.png'), renderBadge(classic, 96));
emit(
	join(staticDir, 'manifest.webmanifest'),
	Buffer.from(JSON.stringify(manifest('classic'), null, '\t') + '\n')
);
emit(join(staticDir, 'wordmark.svg'), Buffer.from(wordmark()));

function manifest(id) {
	const prefix = id === 'classic' ? '' : `mascots/${id}-`;
	const icon = (file) => `/${prefix}${id === 'classic' ? '' : ''}${file}`;
	const base = id === 'classic' ? '/icon-' : `/mascots/${id}-`;
	void icon;
	void prefix;

	return {
		name: 'Frog — personal cinema',
		short_name: 'Frog',
		description: 'Your self-hosted film and TV diary.',
		id: '/',
		start_url: '/',
		scope: '/',
		display: 'standalone',
		orientation: 'portrait-primary',
		background_color: POND_BG,
		theme_color: POND_BG,
		categories: ['entertainment', 'utilities'],
		icons: [
			{ src: `${base}192.png`, sizes: '192x192', type: 'image/png' },
			{ src: `${base}512.png`, sizes: '512x512', type: 'image/png' },
			{ src: `${base}maskable-512.png`, sizes: '512x512', type: 'image/png', purpose: 'maskable' }
		],
		shortcuts: [
			{ name: 'Search', url: '/search' },
			{ name: 'Library', url: '/library' }
		]
	};
}

/** The wordmark: mascot plus "Frog" set in the app's own type stack. */
function wordmark() {
	const mark = VARIANTS.classic
		.build(POND)
		.map((s) => s)
		.map((s) => s);
	const inner = toSvg(mark)
		.replace(/^<svg[^>]*>\n?/, '')
		.replace(/<\/svg>\n?$/, '')
		.trim();

	return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 230 64" width="230" height="64" role="img" aria-label="Frog">
  <g transform="translate(2 2) scale(0.94)">
    ${inner}
  </g>
  <text x="76" y="46" font-family="Segoe UI Variable Display, Segoe UI, -apple-system, Helvetica, Arial, sans-serif"
        font-size="40" font-weight="700" letter-spacing="-1.2" fill="${POND.skin}">Frog</text>
</svg>
`;
}

console.log(`wrote ${written.length} files:`);
for (const f of written) console.log('  ' + f);
