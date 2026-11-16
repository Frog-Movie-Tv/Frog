/**
 * Drawing primitives for the badges, and the medallion every badge sits in.
 *
 * Everything is drawn on a 1024 grid, centre 512. A badge is a medallion
 * (ring, dark pond-green disc) with a scene inside; a step of a ladder also
 * carries its number on a ribbon. Numbers are drawn as strokes, never text,
 * so a badge looks the same wherever it is rasterised.
 */

import { POND } from '../mascots.mjs';

export const C = {
	...POND,
	ink: '#11231a',
	night: '#0f2419',
	cream: '#f5f1e4',
	paper: '#fffaf0',
	red: '#e4572e',
	redDeep: '#b8411f',
	orange: '#f27a33',
	yellow: '#ffd25e',
	gold: '#f2c444',
	goldDeep: '#b07d12',
	teal: '#35b7c9',
	water: '#1f6b66',
	waterDeep: '#154a47',
	pad: '#4ea85f',
	padDeep: '#2f7d46',
	pink: '#ff8fb1',
	purple: '#8f5bd6',
	blue: '#3f7fe0',
	grey: '#9fb3a6',
	white: '#ffffff'
};

export const f = (n) => Math.round(n * 100) / 100;

let defs = new Map();

/** Start a new badge: the gradients each picture needs are collected here. */
export function reset() {
	defs = new Map();
}

export function takeDefs() {
	return [...defs.values()].join('\n    ');
}

/** A gradient (or clip path) by id, kept once per picture; answers its url. */
export function def(id, body) {
	if (!defs.has(id)) defs.set(id, body);
	return `url(#${id})`;
}

export function linear(id, stops, { x1 = 0, y1 = 0, x2 = 0, y2 = 1 } = {}) {
	const s = stops
		.map(
			([o, c, a = 1]) =>
				`<stop offset="${o}" stop-color="${c}"${a !== 1 ? ` stop-opacity="${a}"` : ''}/>`
		)
		.join('');
	return def(
		id,
		`<linearGradient id="${id}" x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}">${s}</linearGradient>`
	);
}

export function radial(id, stops, { cx = 0.5, cy = 0.5, r = 0.5 } = {}) {
	const s = stops
		.map(
			([o, c, a = 1]) =>
				`<stop offset="${o}" stop-color="${c}"${a !== 1 ? ` stop-opacity="${a}"` : ''}/>`
		)
		.join('');
	return def(id, `<radialGradient id="${id}" cx="${cx}" cy="${cy}" r="${r}">${s}</radialGradient>`);
}

const extra = (o) =>
	(o?.opacity !== undefined ? ` opacity="${o.opacity}"` : '') +
	(o?.stroke ? ` stroke="${o.stroke}" stroke-width="${o.sw ?? 4}" stroke-linejoin="round"` : '') +
	(o?.transform ? ` transform="${o.transform}"` : '');

export const ellipse = (cx, cy, rx, ry, fill, o) =>
	`<ellipse cx="${f(cx)}" cy="${f(cy)}" rx="${f(rx)}" ry="${f(ry)}" fill="${fill}"${extra(o)}/>`;
export const circle = (cx, cy, r, fill, o) =>
	`<circle cx="${f(cx)}" cy="${f(cy)}" r="${f(r)}" fill="${fill}"${extra(o)}/>`;
export const rect = (x, y, w, h, r, fill, o) =>
	`<rect x="${f(x)}" y="${f(y)}" width="${f(w)}" height="${f(h)}" rx="${f(r)}" fill="${fill}"${extra(o)}/>`;
export const path = (d, fill, o) => `<path d="${d}" fill="${fill}"${extra(o)}/>`;
export const line = (d, color, width, o = {}) =>
	`<path d="${d}" fill="none" stroke="${color}" stroke-width="${f(width)}" stroke-linecap="${o.cap ?? 'round'}" stroke-linejoin="round"${o.dash ? ` stroke-dasharray="${o.dash}"` : ''}${o.opacity !== undefined ? ` opacity="${o.opacity}"` : ''}/>`;
export const poly = (pts, fill, o) =>
	`<polygon points="${pts.map(([x, y]) => `${f(x)},${f(y)}`).join(' ')}" fill="${fill}"${extra(o)}/>`;

/** Place drawn content: moved to (x, y), turned by `r` degrees, scaled by `s`. */
export function at(x, y, s = 1, r = 0, inner = '') {
	const t = [`translate(${f(x)} ${f(y)})`];
	if (r) t.push(`rotate(${f(r)})`);
	if (s !== 1) t.push(Array.isArray(s) ? `scale(${f(s[0])} ${f(s[1])})` : `scale(${f(s)})`);
	return `<g transform="${t.join(' ')}">${inner}</g>`;
}

export const group = (inner, o = {}) => `<g${extra(o)}>${inner}</g>`;

/** Points of a star: `n` points, outer radius `r`, inner `r * k`. */
export function starPoints(n, r, k = 0.45, turn = -90) {
	const pts = [];
	for (let i = 0; i < n * 2; i++) {
		const a = ((turn + (i * 180) / n) * Math.PI) / 180;
		const rr = i % 2 ? r * k : r;
		pts.push([Math.cos(a) * rr, Math.sin(a) * rr]);
	}
	return pts;
}

// --- rings ---------------------------------------------------------------

/** The ring's metal: four stops from light to deep, and the colour of its dots. */
export const RINGS = {
	lily: { stops: ['#c4f2a6', '#6cc46b', '#2f7d46', '#9be27a'], dots: '#1d4d2c', text: '#d9f7c8' },
	bronze: { stops: ['#f8d3a6', '#d08c4e', '#8c5225', '#e5a76a'], dots: '#5e3416', text: '#f8d3a6' },
	silver: { stops: ['#f7f9fb', '#c3cad2', '#7b8590', '#dfe5eb'], dots: '#3e4650', text: '#eef2f6' },
	gold: { stops: ['#fff3b8', '#f2c444', '#a8740c', '#ffe07a'], dots: '#5c3f04', text: '#ffe68a' },
	jade: { stops: ['#d2f7e3', '#3fbf86', '#16614a', '#86e0b4'], dots: '#0b3626', text: '#bff0d6' },
	ruby: { stops: ['#ffd0d6', '#e2405a', '#86142c', '#ff8a9a'], dots: '#45061a', text: '#ffc2cb' },
	sapphire: {
		stops: ['#d4e4ff', '#4b86e6', '#1a3a86', '#93b9ff'],
		dots: '#0b1d4a',
		text: '#c9dcff'
	},
	amethyst: {
		stops: ['#efdfff', '#a26de6', '#542592', '#d2adff'],
		dots: '#2b0d4d',
		text: '#e4ccff'
	},
	diamond: {
		stops: ['#ffffff', '#d9f3ff', '#8fbcdf', '#f4fcff'],
		dots: '#3f6f91',
		text: '#ffffff'
	},
	aurora: {
		stops: ['#ffd6f2', '#9eeaff', '#8af0b4', '#fff0a0', '#ffb4a8'],
		dots: '#2b2350',
		text: '#ffffff'
	}
};

/** Ladders climb through these, a step at a time; a first step stays lily. */
export const LADDER_RINGS = [
	'bronze',
	'silver',
	'gold',
	'jade',
	'ruby',
	'sapphire',
	'amethyst',
	'diamond',
	'aurora'
];

function ringGradient(name) {
	const ring = RINGS[name];
	const n = ring.stops.length;
	return linear(
		`ring-${name}`,
		ring.stops.map((c, i) => [i / (n - 1), c]),
		{ x1: 0.15, y1: 0.05, x2: 0.85, y2: 0.95 }
	);
}

/** Small four-pointed glints, for the precious rings. */
export function glint(x, y, r, fill = '#ffffff', opacity = 0.9) {
	const k = r * 0.22;
	return path(
		`M${f(x)} ${f(y - r)} Q${f(x + k)} ${f(y - k)} ${f(x + r)} ${f(y)} Q${f(x + k)} ${f(y + k)} ${f(x)} ${f(y + r)} Q${f(x - k)} ${f(y + k)} ${f(x - r)} ${f(y)} Q${f(x - k)} ${f(y - k)} ${f(x)} ${f(y - r)} Z`,
		fill,
		{ opacity }
	);
}

export function medallionBack(ringName) {
	const ring = RINGS[ringName];
	const metal = ringGradient(ringName);
	const disc = radial(
		'disc',
		[
			[0, '#2f6545'],
			[0.6, '#18392a'],
			[1, '#0f2419']
		],
		{ cx: 0.5, cy: 0.36, r: 0.68 }
	);
	let out = circle(512, 512, 502, metal);
	out += `<circle cx="512" cy="512" r="472" fill="none" stroke="${ring.dots}" stroke-opacity="0.45" stroke-width="14" stroke-linecap="round" stroke-dasharray="0 46.3"/>`;
	out += circle(512, 512, 446, ring.dots, { opacity: 0.55 });
	out += circle(512, 512, 438, disc);
	out += `<circle cx="512" cy="512" r="438" fill="none" stroke="#ffffff" stroke-opacity="0.1" stroke-width="6"/>`;
	return out;
}

export function medallionFront(ringName) {
	// A soft shine across the top of the ring, and glints on the rarest.
	let out = `<path d="M150 300 A402 402 0 0 1 874 300" fill="none" stroke="#ffffff" stroke-opacity="0.22" stroke-width="18" stroke-linecap="round"/>`;
	if (ringName === 'diamond' || ringName === 'aurora') {
		out += glint(180, 250, 34) + glint(860, 300, 26) + glint(820, 840, 30) + glint(210, 800, 22);
	}
	if (ringName === 'gold') {
		out += glint(190, 240, 26, '#fffbe0') + glint(845, 820, 22, '#fffbe0');
	}
	return out;
}

// --- numbers -------------------------------------------------------------

/** Stroke glyphs on a 100-high box. Width is how far the next one starts. */
const GLYPHS = {
	0: { w: 60, d: 'M30 0 C8 0 0 22 0 50 C0 78 8 100 30 100 C52 100 60 78 60 50 C60 22 52 0 30 0 Z' },
	1: { w: 36, d: 'M4 18 L28 0 L28 100' },
	2: { w: 60, d: 'M3 22 C5 7 17 0 30 0 C46 0 57 10 57 26 C57 44 40 58 3 100 L59 100' },
	3: {
		w: 60,
		d: 'M4 12 C11 4 20 0 30 0 C46 0 56 9 56 24 C56 40 44 48 27 48 C46 48 58 58 58 74 C58 92 45 100 30 100 C17 100 7 95 1 86'
	},
	4: { w: 62, d: 'M46 100 L46 0 L2 68 L62 68' },
	5: {
		w: 60,
		d: 'M55 0 L11 0 L6 45 C14 39 22 37 31 37 C48 37 58 51 58 68 C58 88 46 100 28 100 C16 100 7 94 1 86'
	},
	6: {
		w: 60,
		d: 'M52 8 C46 3 39 0 32 0 C12 0 2 24 2 54 C2 84 13 100 31 100 C48 100 58 87 58 70 C58 53 46 42 31 42 C16 42 5 52 2 62'
	},
	7: { w: 58, d: 'M0 0 L58 0 L20 100' },
	8: {
		w: 60,
		d: 'M30 46 C15 46 6 36 6 23 C6 9 16 0 30 0 C44 0 54 9 54 23 C54 36 45 46 30 46 C13 46 2 58 2 73 C2 90 14 100 30 100 C46 100 58 90 58 73 C58 58 47 46 30 46 Z'
	},
	9: {
		w: 60,
		d: 'M8 92 C14 98 21 100 28 100 C48 100 58 76 58 46 C58 16 47 0 29 0 C12 0 2 13 2 30 C2 47 14 58 29 58 C44 58 55 48 58 38'
	},
	K: { w: 58, d: 'M4 0 L4 100 M56 0 L6 60 M24 40 L58 100' },
	h: { w: 56, d: 'M4 0 L4 100 M4 62 C10 47 20 40 31 40 C46 40 54 50 54 66 L54 100' }
};

/** A number drawn in strokes, centred on (cx, cy), `height` tall. */
export function label(text, cx, cy, height, color, weight = 19) {
	const gap = 22;
	const glyphs = [...String(text)].map((ch) => GLYPHS[ch]).filter(Boolean);
	const width = glyphs.reduce((a, g) => a + g.w, 0) + gap * (glyphs.length - 1);
	const scale = height / 100;
	let x = cx - (width * scale) / 2;
	let out = '';
	for (const g of glyphs) {
		out += `<path d="${g.d}" transform="translate(${f(x)} ${f(cy - height / 2)}) scale(${f(scale)})" fill="none" stroke="${color}" stroke-width="${weight}" stroke-linecap="round" stroke-linejoin="round"/>`;
		x += (g.w + gap) * scale;
	}
	return { svg: out, width: width * scale };
}

/** The ribbon a ladder's step carries its number on, across the bottom. */
export function ribbon(ringName, text) {
	const ring = RINGS[ringName];
	const metal = ringGradient(ringName);
	// Big enough to read on a profile, where a badge is 28 pixels across.
	const height = text.length > 4 ? 70 : 84;
	const num = label(text, 512, 822, height, ring.text, 21);
	const half = Math.max(140, num.width / 2 + 54);
	const l = 512 - half;
	const r = 512 + half;
	let out = '';
	// The tails, folded behind.
	out += poly(
		[
			[l + 20, 784],
			[l - 58, 798],
			[l - 30, 836],
			[l - 62, 878],
			[l + 20, 864]
		],
		ring.dots
	);
	out += poly(
		[
			[r - 20, 784],
			[r + 58, 798],
			[r + 30, 836],
			[r + 62, 878],
			[r - 20, 864]
		],
		ring.dots
	);
	out += path(`M${l} 764 Q512 784 ${r} 764 L${r} 872 Q512 892 ${l} 872 Z`, '#17291f', {
		stroke: metal,
		sw: 10
	});
	out += num.svg;
	return out;
}

/** A whole badge from its scene. */
export function badge({ ring, scene, number = null, note = '' }) {
	reset();
	const back = medallionBack(ring);
	const front = medallionFront(ring);
	const body = scene();
	const plate = number ? ribbon(ring, number) : '';
	return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
  <!-- ${note} -->
  <defs>
    ${takeDefs()}
  </defs>
  ${back}
  ${body}
  ${plate}
  ${front}
</svg>
`;
}
