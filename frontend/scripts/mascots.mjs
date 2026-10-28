/**
 * The Frog mascot, defined once as geometry.
 *
 * Both the SVG (favicon, in-app logo) and the PNG PWA icons are generated
 * from these primitives, so the 16px favicon and the 512px Android icon can
 * never drift apart. Everything is laid out on a 64x64 grid.
 *
 * Legibility at 16px drove the shape: a wide head, two eye bumps breaking the
 * silhouette at the top, and nothing thinner than about 3 units.
 */

export const POND = {
	skin: '#6cc46b',
	skinDeep: '#4ea85f',
	belly: '#8ad88a',
	eyeWhite: '#fbfaf1',
	pupil: '#11231a',
	shine: '#ffffff',
	blush: '#f4738f',
	mouth: '#2c6b40',
	popcornBox: '#f5f1e4',
	popcornStripe: '#e4572e',
	popcornKernel: '#ffe9a8',
	lensRed: '#e4572e',
	lensCyan: '#35b7c9',
	frame: '#23342b',
	bow: '#ff8fb1',
	bowKnot: '#e5577f',
	leaf: '#9be27a',
	// The belly as it looks laid over the skin, for shapes that cut into it.
	bellyOnSkin: '#7acd79',
	stem: '#3f8f52'
};

/** The head, eye bumps and cheeks: shared by every variant. */
function base(c) {
	return [
		{ type: 'ellipse', cx: 32, cy: 38, rx: 25, ry: 20, fill: c.skin },
		{ type: 'ellipse', cx: 32, cy: 45, rx: 15.5, ry: 11, fill: c.belly, opacity: 0.45 },
		{ type: 'circle', cx: 19, cy: 20, r: 12, fill: c.skin },
		{ type: 'circle', cx: 45, cy: 20, r: 12, fill: c.skin },
		{ type: 'ellipse', cx: 12.5, cy: 40, rx: 5, ry: 3.3, fill: c.blush, opacity: 0.82 },
		{ type: 'ellipse', cx: 51.5, cy: 40, rx: 5, ry: 3.3, fill: c.blush, opacity: 0.82 }
	];
}

/** Sample a quadratic bezier into a polyline, for stroked curves. */
function quad(x0, y0, cx, cy, x1, y1, steps = 14) {
	const pts = [];
	for (let i = 0; i <= steps; i++) {
		const t = i / steps;
		const u = 1 - t;
		pts.push([u * u * x0 + 2 * u * t * cx + t * t * x1, u * u * y0 + 2 * u * t * cy + t * t * y1]);
	}
	return pts;
}

const smile = (c) => ({
	type: 'stroke',
	points: quad(23, 41.5, 32, 50.5, 41, 41.5),
	width: 3,
	color: c.mouth
});

const openEyes = (c) => [
	{ type: 'circle', cx: 19, cy: 20, r: 8, fill: c.eyeWhite },
	{ type: 'circle', cx: 45, cy: 20, r: 8, fill: c.eyeWhite },
	{ type: 'circle', cx: 20.2, cy: 21, r: 4.2, fill: c.pupil },
	{ type: 'circle', cx: 43.8, cy: 21, r: 4.2, fill: c.pupil },
	{ type: 'circle', cx: 18.2, cy: 18.8, r: 1.7, fill: c.shine },
	{ type: 'circle', cx: 41.8, cy: 18.8, r: 1.7, fill: c.shine }
];

export const VARIANTS = {
	classic: {
		label: 'Classic',
		hint: 'Wide awake',
		build: (c = POND) => [...base(c), ...openEyes(c), smile(c)]
	},

	sleepy: {
		label: 'Sleepy',
		hint: 'Just one more episode',
		build: (c = POND) => [
			...base(c),
			{ type: 'circle', cx: 19, cy: 20, r: 8, fill: c.eyeWhite },
			{ type: 'circle', cx: 45, cy: 20, r: 8, fill: c.eyeWhite },
			// Closed lids: a downward arc reads as "asleep" even very small.
			{
				type: 'stroke',
				points: quad(13.5, 19.5, 19, 25.5, 24.5, 19.5),
				width: 2.8,
				color: c.pupil
			},
			{
				type: 'stroke',
				points: quad(39.5, 19.5, 45, 25.5, 50.5, 19.5),
				width: 2.8,
				color: c.pupil
			},
			{ type: 'stroke', points: quad(26, 44, 32, 47.5, 38, 44), width: 2.8, color: c.mouth }
		]
	},

	popcorn: {
		label: 'Popcorn',
		hint: 'Settled in for a double bill',
		build: (c = POND) => [
			...base(c),
			...openEyes(c),
			smile(c),
			// A tub tucked into the lower right, big enough to read at 32px.
			{ type: 'circle', cx: 48, cy: 46, r: 3.4, fill: c.popcornKernel },
			{ type: 'circle', cx: 53, cy: 44.5, r: 3, fill: c.popcornKernel },
			{ type: 'circle', cx: 50.5, cy: 42.5, r: 2.8, fill: c.popcornKernel },
			{ type: 'rect', x: 44, y: 46, w: 13, h: 13, r: 2.2, fill: c.popcornBox },
			{ type: 'rect', x: 48, y: 46, w: 2.6, h: 13, r: 0.6, fill: c.popcornStripe },
			{ type: 'rect', x: 53, y: 46, w: 2.6, h: 13, r: 0.6, fill: c.popcornStripe }
		]
	},

	cinema: {
		label: '3D glasses',
		hint: 'Front row, centre',
		build: (c = POND) => [
			...base(c),
			...openEyes(c),
			smile(c),
			{ type: 'rect', x: 9, y: 14, w: 20, h: 13, r: 4, fill: c.lensRed, opacity: 0.78 },
			{ type: 'rect', x: 35, y: 14, w: 20, h: 13, r: 4, fill: c.lensCyan, opacity: 0.78 },
			{ type: 'rect', x: 28, y: 19, w: 8, h: 2.6, r: 1.3, fill: c.frame },
			{
				type: 'stroke',
				points: [
					[9, 20.5],
					[4, 22]
				],
				width: 2.4,
				color: c.frame
			},
			{
				type: 'stroke',
				points: [
					[55, 20.5],
					[60, 22]
				],
				width: 2.4,
				color: c.frame
			}
		]
	},

	// The kawaii set: big shiny eyes, rosy cheeks, small mouths. Each still has
	// to read at 16px, so the extras are blobs, never fine lines.

	starry: {
		label: 'Starry-eyed',
		hint: 'Big eyes for big moments',
		build: (c = POND) => [
			...base(c),
			// Fuller cheeks than the others.
			{ type: 'ellipse', cx: 12.5, cy: 40, rx: 6, ry: 3.8, fill: c.blush, opacity: 0.95 },
			{ type: 'ellipse', cx: 51.5, cy: 40, rx: 6, ry: 3.8, fill: c.blush, opacity: 0.95 },
			{ type: 'circle', cx: 19, cy: 20, r: 8.5, fill: c.eyeWhite },
			{ type: 'circle', cx: 45, cy: 20, r: 8.5, fill: c.eyeWhite },
			{ type: 'circle', cx: 19.6, cy: 21, r: 6.2, fill: c.pupil },
			{ type: 'circle', cx: 44.4, cy: 21, r: 6.2, fill: c.pupil },
			// Two shines per eye is what makes them sparkle.
			{ type: 'circle', cx: 17.2, cy: 18.4, r: 2.4, fill: c.shine },
			{ type: 'circle', cx: 42, cy: 18.4, r: 2.4, fill: c.shine },
			{ type: 'circle', cx: 21.8, cy: 23.6, r: 1.2, fill: c.shine },
			{ type: 'circle', cx: 46.6, cy: 23.6, r: 1.2, fill: c.shine },
			// A little "ω".
			{
				type: 'stroke',
				points: [
					...quad(26, 43, 29, 47.5, 32, 43.5, 7),
					...quad(32, 43.5, 35, 47.5, 38, 43, 7).slice(1)
				],
				width: 2.6,
				color: c.mouth
			}
		]
	},

	bow: {
		label: 'Bow',
		hint: 'Dressed up for the premiere',
		build: (c = POND) => [
			...base(c),
			...openEyes(c),
			smile(c),
			// The bow sits in the dip between the eyes, so fill the dip first.
			{ type: 'rect', x: 27, y: 12, w: 10, h: 9, r: 2, fill: c.skin },
			{ type: 'ellipse', cx: 25.5, cy: 11.5, rx: 6.5, ry: 4.8, fill: c.bow },
			{ type: 'ellipse', cx: 38.5, cy: 11.5, rx: 6.5, ry: 4.8, fill: c.bow },
			{ type: 'circle', cx: 32, cy: 13, r: 4.2, fill: c.bowKnot }
		]
	},

	delighted: {
		label: 'Delighted',
		hint: 'Best episode yet',
		build: (c = POND) => [
			...base(c),
			// Eyes shut with joy: an upward arc on each bump.
			{
				type: 'stroke',
				points: quad(13, 22.5, 19, 14.5, 25, 22.5),
				width: 3.2,
				color: c.pupil
			},
			{
				type: 'stroke',
				points: quad(39, 22.5, 45, 14.5, 51, 22.5),
				width: 3.2,
				color: c.pupil
			},
			// A wide grin: an oval with its top half cut flat by the belly.
			{ type: 'ellipse', cx: 32, cy: 42.5, rx: 7.5, ry: 6.5, fill: c.mouth },
			{ type: 'rect', x: 24.5, y: 35.6, w: 15, h: 6.9, r: 0, fill: c.bellyOnSkin },
			{ type: 'ellipse', cx: 32, cy: 46.4, rx: 3.8, ry: 2.2, fill: c.blush }
		]
	},

	sprout: {
		label: 'Sprout',
		hint: 'Freshly picked',
		build: (c = POND) => [
			...base(c),
			{ type: 'stroke', points: quad(32, 19, 31, 13, 32, 7), width: 2.4, color: c.stem },
			{ type: 'ellipse', cx: 26.5, cy: 6.5, rx: 5.5, ry: 3.4, fill: c.leaf },
			{ type: 'ellipse', cx: 37.5, cy: 6.5, rx: 5.5, ry: 3.4, fill: c.leaf },
			...openEyes(c),
			smile(c)
		]
	}
};

export const VARIANT_IDS = Object.keys(VARIANTS);

/** Render primitives to an SVG string. */
export function toSvg(shapes, { size = 64, background = null, monochrome = null } = {}) {
	const body = shapes
		.map((s) => {
			const fill = monochrome ?? s.fill ?? 'none';
			const op = s.opacity !== undefined && monochrome === null ? ` opacity="${s.opacity}"` : '';
			switch (s.type) {
				case 'ellipse':
					return `<ellipse cx="${s.cx}" cy="${s.cy}" rx="${s.rx}" ry="${s.ry}" fill="${fill}"${op}/>`;
				case 'circle':
					return `<circle cx="${s.cx}" cy="${s.cy}" r="${s.r}" fill="${fill}"${op}/>`;
				case 'rect':
					return `<rect x="${s.x}" y="${s.y}" width="${s.w}" height="${s.h}" rx="${s.r}" fill="${fill}"${op}/>`;
				case 'stroke': {
					const pts = s.points.map(([x, y]) => `${round(x)},${round(y)}`).join(' ');
					const color = monochrome ?? s.color;
					return `<polyline points="${pts}" fill="none" stroke="${color}" stroke-width="${s.width}" stroke-linecap="round" stroke-linejoin="round"${op}/>`;
				}
				default:
					return '';
			}
		})
		.join('\n  ');

	const bg = background ? `<rect width="64" height="64" rx="14" fill="${background}"/>\n  ` : '';

	return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="${size}" height="${size}" role="img" aria-label="Frog">
  ${bg}${body}
</svg>
`;
}

function round(n) {
	return Math.round(n * 100) / 100;
}
