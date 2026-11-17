/**
 * One scene per badge, by its catalogue key. A ladder's step leaves the
 * bottom of the disc to its ribbon (keep above y = 760); a badge on its own
 * may use the whole disc.
 */

import {
	C,
	at,
	circle,
	def,
	ellipse,
	f,
	glint,
	group,
	label,
	line,
	linear,
	path,
	poly,
	radial,
	rect
} from './draw.mjs';
import {
	apart,
	armchair,
	beam,
	blossom,
	bookSpine,
	burst,
	cake,
	calendar,
	cards,
	carpet,
	check,
	chest,
	chili,
	clapper,
	clock,
	crown,
	cushion,
	directorChair,
	filmStrip,
	flag,
	flame,
	fly,
	frog,
	gavel,
	gem,
	heart,
	hourglass,
	infinity,
	laurel,
	lilyPad,
	loopArrow,
	magnifier,
	mask,
	mic,
	moon,
	mug,
	openBook,
	peek,
	pencil,
	petals,
	plane,
	pond,
	popcorn,
	poster,
	POSTER_COLOURS,
	projector,
	pumpkin,
	puzzle,
	putFrog,
	reel,
	remote,
	road,
	screens,
	seat,
	shelf,
	shipWheel,
	shoe,
	snail,
	snow,
	sofa,
	sparkles,
	spotlight,
	sprout,
	star,
	stopwatch,
	studio,
	sun,
	tadpole,
	telescope,
	throne,
	ticket,
	tower,
	trophy,
	tv,
	lotus
} from './parts.mjs';

// --- helpers --------------------------------------------------------------------

function glow(x, y, r, color = '#ffe9a8', a = 0.3) {
	const id = `glow-${color.replace('#', '')}-${Math.round(a * 100)}`;
	return circle(
		x,
		y,
		r,
		radial(id, [
			[0, color, a],
			[1, color, 0]
		])
	);
}

const shadow = (x, y, rx, ry = rx * 0.12, a = 0.3) => ellipse(x, y, rx, ry, '#000', { opacity: a });

/** Clip anything to the disc, for scenes with a background of their own. */
function inDisc(inner) {
	def('discclip', '<clipPath id="discclip"><circle cx="512" cy="512" r="436"/></clipPath>');
	return `<g clip-path="url(#discclip)">${inner}</g>`;
}

/** Night sky behind a scene. */
function sky(top = '#14243d', bottom = '#1d3a4f') {
	return inDisc(
		rect(
			60,
			60,
			904,
			904,
			0,
			linear(`sky-${top.slice(1)}`, [
				[0, top],
				[1, bottom]
			])
		)
	);
}

function stars(points) {
	return points
		.map(([x, y, r]) =>
			r > 9 ? glint(x, y, r, '#fff7cc') : circle(x, y, r, '#fff7cc', { opacity: 0.8 })
		)
		.join('');
}

const motion = (x, y, n = 3, len = 70, gap = 26, color = '#ffffff') =>
	Array.from({ length: n }, (_, i) =>
		line(`M${x} ${y + i * gap} L${x - len + i * 14} ${y + i * gap}`, color, 10, { opacity: 0.45 })
	).join('');

/** A pond and lily pad to stand a frog on. */
const padBase = (y, rx = 240, o = {}) =>
	at(512, y + 30, 1, 0, pond(rx + 90, 70, { ripples: true })) +
	at(512, y, 1, 0, lilyPad(rx, rx * 0.27, o));

const zed = (x, y, s) =>
	line(`M${x} ${y} L${x + s} ${y} L${x} ${y + s} L${x + s} ${y + s}`, '#cfe9ff', s * 0.18);

const note = (x, y, s = 1, color = '#fff3c4') =>
	at(
		x,
		y,
		s,
		0,
		line('M0 0 L0 -60 L34 -70 L34 -10', color, 8) +
			ellipse(-10, 0, 14, 10, color) +
			ellipse(24, -10, 14, 10, color)
	);

const confetti = (items) =>
	items.map(([x, y, c, r = 0]) => at(x, y, 1, r, rect(-9, -5, 18, 10, 3, c))).join('');

const GOLD_FROG = {
	skin: '#f2c444',
	skinDeep: '#b88a16',
	belly: '#ffe68a',
	eyeWhite: '#fff3b8',
	pupil: '#7a5410',
	shine: '#ffffff',
	blush: '#f2c444',
	mouth: '#7a5410'
};
const GREY_FROG = {
	skin: '#b9c0c8',
	skinDeep: '#7b8590',
	belly: '#dfe5eb',
	eyeWhite: '#ffffff',
	pupil: '#23342b',
	blush: '#9aa3ad',
	mouth: '#3e4650'
};
const ROBO_FROG = {
	skin: '#9fb3c4',
	skinDeep: '#6f8396',
	belly: '#d6e2ec',
	blush: '#7fd3ff',
	mouth: '#2b3d4d'
};
const BLUE_FROG = { skin: '#5fb3d6', skinDeep: '#3f8fb0', belly: '#9fd8ee' };
const PINK_FROG = { skin: '#f79ab8', skinDeep: '#d97597', belly: '#ffc4d8', blush: '#ff6b8f' };

// --- the scenes -----------------------------------------------------------------

export const SCENES = {
	// films ---------------------------------------------------------------------
	'films.1': () => {
		let out = glow(512, 470, 330, '#8ff0ff', 0.18);
		out += at(512, 650, 1, 0, pond(300, 70));
		const egg = (x, y, r) =>
			circle(x, y, r, '#d7f5ff', { opacity: 0.4 }) +
			circle(x, y, r, 'none', { stroke: '#ffffff', sw: 4, opacity: 0.45 }) +
			circle(x + r * 0.08, y + r * 0.1, r * 0.34, '#23342b') +
			circle(x - r * 0.35, y - r * 0.4, r * 0.14, '#ffffff', { opacity: 0.6 });
		out +=
			egg(330, 560, 62) +
			egg(700, 560, 66) +
			egg(400, 640, 48) +
			egg(626, 652, 52) +
			egg(752, 450, 44) +
			egg(278, 440, 46);
		out += circle(512, 470, 160, '#d7f5ff', { opacity: 0.38 });
		out += circle(512, 470, 160, 'none', { stroke: '#ffffff', sw: 6, opacity: 0.55 });
		out += at(512, 480, 0.62, 0, filmStrip(260, 150, { frames: 1, frame: '#3e7d6a' }));
		out += poly(
			[
				[488, 446],
				[548, 480],
				[488, 514]
			],
			'#ffffff'
		);
		out += circle(450, 390, 22, '#ffffff', { opacity: 0.55 });
		out += sparkles([
			[690, 300, 30],
			[330, 320, 20]
		]);
		return out;
	},
	'films.10': () => {
		let out = at(512, 560, 1, 0, pond(340, 190, { ripples: false }));
		out +=
			circle(330, 470, 16, 'none', { stroke: '#cfe9ff', sw: 4, opacity: 0.6 }) +
			circle(290, 420, 10, 'none', { stroke: '#cfe9ff', sw: 4, opacity: 0.6 });
		out += at(
			560,
			540,
			1.7,
			-8,
			tadpole() +
				rect(4, -24, 22, 18, 5, C.lensRed, { opacity: 0.85 }) +
				rect(24, -18, 20, 18, 5, C.lensCyan, { opacity: 0.85 }) +
				line('M26 -14 L24 -14', C.frame, 3)
		);
		out += at(690, 420, 0.95, 22, ticket(170, 86, { color: C.yellow }));
		out += sparkles([
			[420, 340, 26],
			[770, 600, 20]
		]);
		return out;
	},
	'films.50': () => {
		let out = glow(512, 470, 320, '#ffe9a8', 0.28);
		out += shadow(512, 702, 150, 18);
		out += at(512, 330, 3.6, 0, peek({ eyes: 'starry' }));
		out += at(512, 520, 0.62, 0, popcorn({ id: 'p50' }));
		out += at(512, 572, 1, 0, star(30, C.yellow));
		return out;
	},
	'films.100': () => {
		let out = inDisc(
			poly(
				[
					[260, 60],
					[764, 60],
					[900, 420],
					[124, 420]
				],
				radial(
					'screenlight',
					[
						[0, '#cfe9ff', 0.35],
						[1, '#cfe9ff', 0]
					],
					{ cx: 0.5, cy: 0, r: 0.9 }
				)
			)
		);
		const s = seat();
		out += at(512, 610, 1.05, 0, s.back);
		out += putFrog(512, 470, 4.3, {
			arms: 'hold',
			eyes: 'starry',
			mouth: 'open',
			wear: ['glasses3d']
		});
		out += at(512, 640, 0.17, 0, popcorn({ id: 'p100', band: false }));
		out += at(512, 610, 1.05, 0, s.front);
		return out;
	},
	'films.250': () => {
		let out = glow(512, 480, 320, '#ffe9a8', 0.22);
		out += at(512, 490, 1, 0, laurel(-1, 300) + laurel(1, 300));
		out += shadow(512, 742, 170, 16);
		out += putFrog(512, 380, 3.5, { wear: ['beret'], mouth: 'grin', eyes: 'happy' });
		out += at(512, 610, 0.95, 0, clapper({ open: -14 }));
		return out;
	},
	'films.500': () => {
		let out = glow(512, 480, 320, '#cfe9ff', 0.16);
		out += at(380, 640, 1, -12, filmStrip(420, 90, { frames: 4 }));
		out += at(560, 590, 1, 0, reel(150));
		out += motion(330, 300, 3, 90, 30);
		out += putFrog(560, 320, 4, { pose: 'leap', eyes: 'happy', mouth: 'open' }, -8);
		out += sparkles([
			[760, 260, 24],
			[260, 480, 18]
		]);
		return out;
	},
	'films.750': () => {
		let out = at(626, 590, 1, 0, beam(260, 170, { id: 'b750' }));
		out += at(
			780,
			590,
			1.6,
			0,
			group(
				peek({
					colors: { skin: '#1a2d23', eyeWhite: '#1a2d23', pupil: '#1a2d23', shine: '#1a2d23' }
				}),
				{ opacity: 0.6 }
			)
		);
		out += shadow(420, 728, 210, 16);
		out += at(420, 560, 0.92, 0, projector());
		out += putFrog(268, 470, 2.5, { arms: 'none', wear: ['headset'], look: [2, 0] });
		return out;
	},
	'films.1000': () => {
		let out = at(512, 140, 1, 0, spotlight(0, 560, 230, { id: 's1000' }));
		out += at(512, 770, 1, 0, carpet(520, 210));
		out += at(512, 640, 1, 0, lilyPad(220, 60, { flower: true, notch: -110 }));
		out += putFrog(512, 432, 3.9, { arms: 'wave', wear: ['bow'], eyes: 'starry', mouth: 'grin' });
		out += sparkles([
			[250, 330, 28],
			[780, 360, 24],
			[300, 560, 16],
			[740, 560, 18]
		]);
		return out;
	},
	'films.1500': () => {
		let out = glow(512, 430, 300, '#ffe68a', 0.35);
		out +=
			rect(372, 640, 280, 110, 14, '#23342b') +
			rect(392, 660, 240, 40, 8, C.gold) +
			rect(352, 728, 320, 34, 10, '#1a2a21');
		out += putFrog(512, 452, 3.6, {
			colors: GOLD_FROG,
			arms: 'up',
			eyes: 'closed',
			mouth: 'small'
		});
		out += at(512, 300, 1, 0, star(50, '#fff3b8'));
		out += sparkles(
			[
				[320, 300, 30],
				[720, 320, 26],
				[300, 600, 18],
				[730, 600, 20]
			],
			'#fff3b8'
		);
		return out;
	},
	'films.2500': () => {
		let out = '';
		for (let i = 0; i < 16; i++) {
			out += at(
				512,
				470,
				1,
				i * 22.5,
				poly(
					[
						[0, 0],
						[-40, -420],
						[40, -420]
					],
					'#fff3c4',
					{ opacity: i % 2 ? 0.05 : 0.11 }
				)
			);
		}
		out = inDisc(out);
		out +=
			at(410, 650, 1, 0, reel(95)) + at(614, 650, 1, 0, reel(95)) + at(512, 568, 1, 0, reel(84));
		out += putFrog(512, 330, 3.4, { arms: 'up', eyes: 'starry', mouth: 'grin', wear: ['crown'] });
		out += sparkles([
			[260, 300, 32],
			[780, 280, 28],
			[800, 520, 20]
		]);
		return out;
	},

	// episodes ------------------------------------------------------------------
	'episodes.1': () => {
		let out = glow(512, 500, 320, '#8ff0ff', 0.2);
		out += at(512, 392, 3.3, 0, peek({ look: [0, 2] }));
		out += at(
			512,
			548,
			1.1,
			0,
			tv(380, 280, {
				antenna: false,
				screen:
					at(-8, 0, 0.55, -12, plane()) +
					line('M-120 50 Q-80 20 -50 40', '#ffffff', 5, { dash: '4 14', opacity: 0.8 })
			})
		);
		return out;
	},
	'episodes.100': () => {
		let out = glow(640, 460, 260, '#8ff0ff', 0.22);
		out += at(
			650,
			470,
			0.85,
			0,
			tv(380, 280, {
				screen: poly(
					[
						[-30, -40],
						[40, 0],
						[-30, 40]
					],
					'#ffffff'
				)
			})
		);
		out += putFrog(330, 520, 3.2, { arms: 'hold', look: [3, -1] });
		out += at(330, 640, 0.15, 0, popcorn({ id: 'p100e', band: false }));
		out += shadow(512, 730, 280, 16);
		return out;
	},
	'episodes.250': () => {
		const s = sofa({ color: '#3f7fe0' });
		let out = glow(512, 300, 260, '#8ff0ff', 0.16);
		out += at(512, 620, 0.95, 0, s.back);
		out += putFrog(512, 440, 3.5, { arms: 'hold', eyes: 'sleepy', mouth: 'small' });
		out += at(512, 576, 0.55, 90, remote());
		out += at(512, 620, 0.95, 0, s.front);
		return out;
	},
	'episodes.500': () => {
		let out = glow(600, 400, 260, '#ff9a8a', 0.18);
		out += shadow(470, 740, 170, 16);
		out += putFrog(430, 500, 4, { arms: 'right', eyes: 'focus', mouth: 'smirk' });
		out += at(560, 360, 1.6, 18, remote());
		out += circle(560, 300, 40, '#ff6b5a', { opacity: 0.35 });
		out += sparkles([
			[700, 260, 26],
			[740, 420, 18]
		]);
		return out;
	},
	'episodes.1000': () => {
		const ch = armchair();
		const spud = radial(
			'spud',
			[
				[0, '#e2b277'],
				[0.7, '#c8955a'],
				[1, '#a8743e']
			],
			{ cx: 0.42, cy: 0.35, r: 0.7 }
		);
		let out = circle(
			512,
			512,
			438,
			radial(
				'tvlight',
				[
					[0, '#35b7c9', 0.35],
					[1, '#35b7c9', 0]
				],
				{ cx: 0.8, cy: 0.35, r: 0.7 }
			)
		);
		out += shadow(512, 812, 250, 22, 0.32);
		out += at(512, 560, 1, 0, ch.back);
		let p = path(
			'M-134 6 C-140 -74 -74 -128 8 -126 C90 -124 146 -76 140 10 C136 86 80 134 0 132 C-82 130 -130 84 -134 6 Z',
			spud
		);
		// A frog hood on the potato: the onesie's eyes sit on top.
		p += path(
			'M-136 -4 C-134 -80 -72 -130 6 -128 C88 -126 144 -82 142 -2 C100 -34 -90 -36 -136 -4 Z',
			C.skin
		);
		p += circle(-56, -118, 40, C.skin) + circle(64, -116, 40, C.skin);
		p += circle(-56, -118, 26, C.eyeWhite) + circle(64, -116, 26, C.eyeWhite);
		p +=
			circle(-50, -114, 13, C.pupil) +
			circle(70, -112, 13, C.pupil) +
			circle(-46, -118, 4, '#ffffff') +
			circle(74, -116, 4, '#ffffff');
		p += path('M-136 -4 C-90 -36 100 -34 142 -2', 'none', { stroke: C.skinDeep, sw: 8 });
		p +=
			ellipse(-70, 36, 9, 6, '#8a5a2c', { opacity: 0.5 }) +
			ellipse(80, 60, 7, 5, '#8a5a2c', { opacity: 0.5 });
		p +=
			path('M-62 16 A26 26 0 0 1 -10 16 Z', '#c08a50') +
			path('M20 16 A26 26 0 0 1 72 16 Z', '#c08a50');
		p += line('M-62 16 L-10 16 M20 16 L72 16', '#7a4d24', 5);
		p +=
			ellipse(-68, 52, 22, 13, C.blush, { opacity: 0.5 }) +
			ellipse(76, 52, 22, 13, C.blush, { opacity: 0.5 });
		p += line('M-20 56 Q4 76 28 56', '#5c3a1a', 7);
		p += ellipse(-42, 140, 30, 18, '#b98448') + ellipse(44, 140, 30, 18, '#b98448');
		out += at(512, 560, 1, 0, p);
		out += at(512, 560, 1, 0, ch.front);
		out += line('M630 596 C660 584 680 560 690 530', '#c8955a', 30);
		out += at(700, 470, 1, -24, remote());
		out += circle(694, 524, 26, '#c8955a');
		return out;
	},
	'episodes.2500': () => {
		let out = inDisc(
			rect(
				60,
				60,
				904,
				904,
				0,
				linear('deep', [
					[0, '#1f6b7a'],
					[1, '#0d2f3f']
				])
			)
		);
		out += inDisc(
			poly(
				[
					[380, 60],
					[470, 60],
					[300, 900],
					[180, 900]
				],
				'#ffffff',
				{ opacity: 0.05 }
			) +
				poly(
					[
						[560, 60],
						[620, 60],
						[560, 900],
						[460, 900]
					],
					'#ffffff',
					{ opacity: 0.05 }
				)
		);
		out += at(330, 300, 0.24, -12, tv(380, 280, { antenna: false, legs: false }));
		out += at(740, 600, 0.2, 14, tv(380, 280, { antenna: false, legs: false }));
		out += putFrog(560, 470, 4, {
			pose: 'swim',
			eyes: 'happy',
			mouth: 'small',
			wear: ['sunglasses']
		});
		for (const [x, y, r] of [
			[660, 330, 18],
			[690, 270, 12],
			[672, 220, 9],
			[420, 620, 14],
			[398, 580, 9]
		])
			out += circle(x, y, r, 'none', { stroke: '#cfe9ff', sw: 4, opacity: 0.7 });
		return out;
	},
	'episodes.5000': () => {
		let out = glow(512, 450, 330, '#b48cff', 0.22);
		out += shadow(512, 744, 160, 16);
		out += putFrog(470, 480, 4, { arms: 'right', eyes: 'starry', mouth: 'grin', wear: ['wizard'] });
		out += at(600, 470, 0.9, 30, remote({ button: C.yellow }));
		out += line('M640 400 Q720 300 690 220 Q680 180 720 150', '#fff3b8', 6, { dash: '2 18' });
		out += sparkles(
			[
				[700, 210, 30],
				[760, 320, 22],
				[640, 260, 16],
				[300, 300, 20]
			],
			'#fff3b8'
		);
		out += at(745, 140, 1, 0, star(26, C.yellow));
		return out;
	},
	'episodes.7500': () => {
		let out = padBase(690, 250);
		out += at(620, 470, 0.85, 0, tower());
		out += putFrog(390, 520, 3, { arms: 'right', wear: ['headset'], mouth: 'open' });
		return out;
	},
	'episodes.10000': () => {
		let out = glow(512, 420, 320, '#ffe68a', 0.25);
		out += shadow(512, 750, 220, 18);
		out += at(512, 560, 0.95, 0, throne());
		out += putFrog(512, 440, 3.3, { arms: 'right', wear: ['crown'], eyes: 'happy', mouth: 'grin' });
		out += at(610, 448, 0.5, 30, remote({ button: C.gold }));
		return out;
	},

	// hours ------------------------------------------------------------------------
	'hours.24': () => {
		const day = linear('dayside', [
			[0, '#ffe9a8'],
			[1, '#ffb14a']
		]);
		const night = linear('nightside', [
			[0, '#2a3f73'],
			[1, '#14243d']
		]);
		let out = shadow(512, 740, 180, 16);
		out += circle(512, 470, 240, '#23342b');
		out +=
			path('M512 250 A220 220 0 0 0 512 690 Z', day) +
			path('M512 250 A220 220 0 0 1 512 690 Z', night);
		out += at(400, 420, 1, 0, sun(40, '#fff3c4'));
		out += at(630, 420, 1, 0, moon(46));
		out += stars([
			[680, 540, 6],
			[600, 560, 4],
			[690, 360, 4]
		]);
		out += circle(512, 470, 220, 'none', { stroke: '#23342b', sw: 12 });
		out +=
			line('M512 470 L512 330', '#23342b', 14) +
			line('M512 470 L620 520', '#23342b', 12) +
			circle(512, 470, 16, C.red);
		out += at(512, 222, 1.8, 0, peek({ eyes: 'closed' }));
		return out;
	},
	'hours.100': () => {
		let out = sky('#162a48', '#1f4a55');
		out += stars([
			[260, 280, 5],
			[340, 220, 3],
			[440, 300, 4],
			[700, 330, 4],
			[600, 230, 3],
			[800, 430, 14]
		]);
		out += at(690, 280, 1, -20, moon(80));
		out += glow(690, 280, 200, '#fff3c4', 0.18);
		out += at(512, 690, 1, 0, pond(420, 120));
		out += ellipse(690, 650, 40, 120, '#fff3c4', { opacity: 0.15 });
		out += at(430, 620, 1, 0, lilyPad(170, 46));
		out += putFrog(430, 470, 2.9, { look: [3, -3], mouth: 'small' });
		out += sparkles(
			[
				[250, 520, 14],
				[300, 440, 10]
			],
			'#d6ff8a'
		);
		return out;
	},
	'hours.250': () => {
		let out = glow(512, 470, 300, '#cfe9ff', 0.14);
		out += shadow(512, 742, 170, 16);
		const inside =
			ellipse(0, 20, 110, 80, C.skin) +
			circle(-58, -30, 50, C.skin) +
			circle(58, -30, 50, C.skin) +
			line('M-84 -28 Q-58 -6 -32 -28 M32 -28 Q58 -6 84 -28', C.pupil, 10) +
			ellipse(-80, 34, 20, 13, C.blush, { opacity: 0.8 }) +
			ellipse(80, 34, 20, 13, C.blush, { opacity: 0.8 }) +
			line('M-24 50 Q0 64 24 50', C.mouth, 9);
		out += at(512, 480, 1, 0, calendar(330, 370, { head: '#3f7fe0', inside }));
		out += zed(640, 250, 50) + zed(710, 190, 36) + zed(760, 150, 26);
		return out;
	},
	'hours.500': () => {
		let out = glow(470, 460, 300, '#ffe9a8', 0.2);
		out += shadow(512, 742, 220, 16);
		out += at(450, 480, 1.2, 0, hourglass());
		out += putFrog(690, 560, 2.6, { look: [-3, -2], mouth: 'o' });
		return out;
	},
	'hours.1000': () => {
		let out = sky('#14243d', '#1d4155');
		out += stars([
			[260, 300, 4],
			[350, 230, 3],
			[760, 260, 4],
			[800, 380, 3],
			[240, 420, 12]
		]);
		out += glow(512, 330, 230, '#fff3c4', 0.22);
		out += at(
			512,
			330,
			1,
			0,
			reel(140, {
				color: radial('moonreel', [
					[0, '#fffbe6'],
					[1, '#f2dc8f']
				]),
				hole: '#c8b56a'
			})
		);
		out += at(512, 700, 1, 0, pond(420, 110));
		out += ellipse(512, 660, 60, 90, '#fff3c4', { opacity: 0.12 });
		out += at(380, 640, 1, 0, lilyPad(130, 36));
		out += putFrog(380, 530, 2.2, {
			colors: { skin: '#2c5a3e', skinDeep: '#1f4630', belly: '#3d7552', blush: '#2c5a3e' },
			look: [2, -3]
		});
		return out;
	},
	'hours.2500': () => {
		let out = sky('#1b2c48', '#2b4a66');
		for (const [x, y, r] of [
			[260, 260, 8],
			[340, 340, 6],
			[700, 220, 9],
			[780, 360, 7],
			[600, 300, 5],
			[420, 220, 6],
			[820, 480, 6]
		])
			out += circle(x, y, r, '#ffffff', { opacity: 0.85 });
		out += glow(690, 560, 220, '#8ff0ff', 0.3);
		out += inDisc(at(512, 760, 1, 0, snow(900)));
		out += at(690, 580, 0.6, 0, tv(380, 280, { antenna: true }));
		out += putFrog(390, 520, 3.5, {
			arms: 'hold',
			eyes: 'happy',
			mouth: 'small',
			wear: ['nightcap', 'scarf'],
			look: [3, 0]
		});
		out += at(390, 652, 0.5, 0, mug('#e4572e'));
		return out;
	},
	'hours.5000': () => {
		let out = glow(512, 470, 330, '#b48cff', 0.18);
		out += at(512, 540, 1, 0, infinity(560, '#23342b'));
		out += putFrog(512, 330, 2.9, { eyes: 'closed', mouth: 'small', wear: ['halo'] });
		out += sparkles([
			[250, 400, 22],
			[780, 400, 22],
			[512, 690, 18]
		]);
		return out;
	},

	// library ------------------------------------------------------------------------
	'library.50': () => {
		let out = padBase(660, 240);
		out +=
			at(400, 510, 1, -10, poster(90, 130, POSTER_COLOURS[0])) +
			at(512, 480, 1, 0, poster(96, 140, POSTER_COLOURS[1])) +
			at(624, 510, 1, 10, poster(90, 130, POSTER_COLOURS[2]));
		out += putFrog(512, 560, 2.3, { mouth: 'grin', eyes: 'happy' });
		return out;
	},
	'library.250': () => {
		let out = shadow(512, 744, 170, 16);
		const stack = [0, 1, 2, 3, 4, 5]
			.map((i) =>
				at(
					512 + (i % 2 ? 10 : -8),
					380 - i * 44,
					1,
					(i % 3) * 4 - 4,
					rect(-110, -20, 220, 40, 8, POSTER_COLOURS[i]) +
						rect(-100, -12, 60, 24, 4, '#ffffff', { opacity: 0.35 })
				)
			)
			.join('');
		out += stack;
		out += putFrog(512, 520, 3.6, { arms: 'up', eyes: 'wide', mouth: 'wobble' });
		out += motion(370, 230, 2, 50, 30);
		return out;
	},
	'library.500': () => {
		let out = glow(512, 470, 320, '#ffe68a', 0.25);
		out += shadow(512, 712, 210, 18);
		out += at(670, 404, 2.4, 14, peek({ eyes: 'starry' }));
		out += at(512, 570, 0.95, 0, chest());
		out += sparkles([
			[300, 320, 26],
			[760, 300, 22],
			[512, 230, 18]
		]);
		return out;
	},
	'library.750': () => {
		let out = shadow(512, 712, 210, 16);
		out += at(512, 520, 1, 0, shelf());
		out += putFrog(600, 216, 2.3, { arms: 'right', mouth: 'open', eyes: 'happy' });
		out +=
			line('M300 700 L360 340 M360 700 L420 340', '#a8642e', 12) +
			[0, 1, 2, 3, 4]
				.map((i) =>
					line(
						`M${f(306 + i * 12)} ${660 - i * 72} L${f(366 + i * 12)} ${660 - i * 72}`,
						'#a8642e',
						8
					)
				)
				.join('');
		return out;
	},
	'library.1000': () => {
		let out = at(512, 610, 1, 0, pond(370, 130));
		out += at(512, 570, 1, 0, lilyPad(320, 110, { notch: -80 }));
		const frogs = [
			[400, 500, 1.7, 'happy'],
			[520, 470, 1.9, 'open'],
			[640, 510, 1.7, 'starry'],
			[350, 590, 1.6, 'open'],
			[470, 600, 1.7, 'happy'],
			[600, 610, 1.7, 'open'],
			[700, 580, 1.5, 'happy']
		];
		for (const [x, y, s, e] of frogs)
			out += putFrog(x, y, s, { eyes: e, mouth: e === 'happy' ? 'grin' : 'smile' });
		return out;
	},
	'library.1500': () => {
		let out = at(512, 560, 1, 0, pond(400, 210, { ripples: false }));
		for (const [x, y, rx, flower] of [
			[330, 470, 90, false],
			[520, 430, 100, true],
			[700, 500, 90, false],
			[430, 620, 100, false],
			[640, 650, 90, true]
		])
			out += at(x, y, 1, 0, lilyPad(rx, rx * 0.3, { flower, notch: (x * 7) % 360 }));
		out +=
			putFrog(330, 420, 1.5, {}) +
			putFrog(700, 450, 1.5, { eyes: 'happy', mouth: 'grin' }) +
			putFrog(430, 572, 1.6, { eyes: 'starry' });
		out +=
			at(560, 360, 0.6, -8, poster(90, 120, POSTER_COLOURS[3])) +
			at(650, 610, 0.5, 10, poster(90, 120, POSTER_COLOURS[1]));
		return out;
	},
	'library.2500': () => {
		let out = glow(512, 470, 330, '#ffe68a', 0.24);
		for (let i = 0; i < 9; i++) {
			const a = ((i * 40 - 90) * Math.PI) / 180;
			out += at(
				512 + Math.cos(a) * 300,
				470 + Math.sin(a) * 250,
				0.75,
				i * 40,
				poster(80, 110, POSTER_COLOURS[i % 7])
			);
		}
		out += at(512, 640, 1, 0, lilyPad(170, 48, { flower: true }));
		out += putFrog(512, 470, 3.1, { arms: 'up', wear: ['crown'], eyes: 'starry', mouth: 'grin' });
		return out;
	},

	// shows completed --------------------------------------------------------------
	'completed.1': () => {
		let out = glow(512, 480, 300, '#8af0b4', 0.2);
		out += at(512, 500, 1.15, 0, tv(380, 280, { screen: at(0, 0, 0.62, 0, check('#ffffff', 36)) }));
		return out;
	},
	'completed.10': () => {
		let out = padBase(690, 220);
		out += putFrog(420, 470, 3.6, { arms: 'right', eyes: 'happy', mouth: 'grin' });
		out += at(526, 495, 1, 12, flag(280, { checkered: true, id: 'c10' }));
		out += confetti([
			[300, 260, C.yellow, 20],
			[700, 230, C.pink, -30],
			[760, 420, C.teal, 10],
			[250, 420, '#8af0b4', 40]
		]);
		return out;
	},
	'completed.25': () => {
		let out = glow(512, 430, 300, '#ffe68a', 0.28);
		out += shadow(512, 744, 150, 14);
		out += at(512, 560, 1.05, 0, trophy());
		out +=
			rect(430, 360, 164, 120, 22, '#23342b') +
			rect(
				442,
				372,
				140,
				96,
				16,
				radial('tvcup', [
					[0, '#8ff0ff'],
					[1, '#1f7f8e']
				])
			);
		out += at(512, 420, 0.35, 0, check('#ffffff', 34));
		out += sparkles([
			[300, 300, 26],
			[740, 300, 22]
		]);
		return out;
	},
	'completed.50': () => {
		let out = line('M230 740 L230 400 M794 740 L794 400', '#f5f1e4', 16);
		out +=
			line('M230 430 Q330 470 420 420', C.red, 12) + line('M604 420 Q700 470 794 430', C.red, 12);
		out += putFrog(520, 420, 3.8, { pose: 'leap', eyes: 'happy', mouth: 'open' }, -12);
		out += motion(300, 300, 3, 70, 28);
		out += confetti([
			[300, 220, C.yellow, 20],
			[700, 200, C.pink, -30],
			[760, 330, C.teal, 10],
			[640, 640, '#8af0b4', 40],
			[380, 640, C.yellow, -20]
		]);
		return out;
	},
	'completed.100': () => {
		let out = padBase(690, 240);
		out += at(620, 660, 1, 0, flag(380, { checkered: true, id: 'c100' }));
		out += putFrog(400, 500, 3.3, { arms: 'wave', eyes: 'happy', mouth: 'grin' });
		return out;
	},
	'completed.250': () => {
		let out = glow(512, 420, 320, '#ffe68a', 0.3);
		out += at(
			512,
			500,
			1,
			0,
			laurel(-1, 300, { color: '#f2c444', dark: '#c99a1e', stem: '#a8740c' }) +
				laurel(1, 300, { color: '#f2c444', dark: '#c99a1e', stem: '#a8740c' })
		);
		out += shadow(512, 750, 150, 14);
		out += at(512, 590, 0.95, 0, trophy());
		out += putFrog(512, 292, 2.6, { arms: 'up', eyes: 'happy', mouth: 'grin' });
		return out;
	},

	// ratings ----------------------------------------------------------------------
	'ratings.10': () => {
		let out = shadow(512, 744, 220, 16);
		out += at(
			640,
			490,
			1,
			8,
			rect(-110, -150, 220, 300, 20, C.paper, { stroke: '#cfc8b6', sw: 4 }) +
				at(0, -30, 1, 0, star(70, C.yellow)) +
				line('M-70 80 L70 80 M-70 110 L30 110', '#cfc8b6', 8)
		);
		out += putFrog(360, 520, 3.4, { arms: 'right', eyes: 'focus', mouth: 'small' });
		out += at(480, 430, 0.55, 140, pencil());
		return out;
	},
	'ratings.50': () => {
		let out = glow(470, 430, 260, '#ffe68a', 0.3);
		out += at(470, 430, 1, -8, star(150, C.yellow, { edge: '#e0a91c' }));
		out += line('M330 600 Q420 650 520 610 Q600 580 650 620', '#fff3c4', 8, { dash: '2 18' });
		out += at(650, 590, 1, -40, pencil());
		out += at(
			400,
			380,
			1.1,
			0,
			frog({
				pose: 'head',
				eyes: 'happy',
				mouth: 'grin',
				colors: { skin: '#ffe68a', skinDeep: '#e0a91c', belly: '#fff3c4', blush: '#ffb14a' }
			})
		);
		return out;
	},
	'ratings.250': () => {
		let out = sky('#14243d', '#1f3d55');
		out += stars([
			[300, 200, 6],
			[420, 260, 4],
			[540, 180, 5],
			[700, 220, 18],
			[780, 330, 6],
			[640, 300, 4]
		]);
		out += at(720, 230, 1, 0, star(40, C.yellow));
		out += at(600, 520, 1, 0, telescope());
		out += at(512, 700, 1, 0, lilyPad(300, 70));
		out += putFrog(380, 540, 3, { look: [3, -3], mouth: 'o', arms: 'right' });
		return out;
	},
	'ratings.500': () => {
		let out = shadow(512, 744, 230, 16);
		out += rect(240, 600, 544, 150, 18, '#8c5225') + rect(260, 620, 504, 20, 6, '#a8642e');
		out += putFrog(512, 430, 3.7, { wear: ['wig'], eyes: 'focus', mouth: 'flat', arms: 'hold' });
		out += at(670, 560, 0.8, 0, gavel());
		out += at(360, 670, 1, 0, star(34, C.yellow));
		return out;
	},
	'ratings.1000': () => {
		let out = rect(200, 190, 624, 420, 26, '#1a2f25');
		const cols = [C.yellow, '#ffb14a', C.pink, '#8ff0ff', '#8af0b4'];
		for (let r = 0; r < 3; r++)
			for (let c = 0; c < 5; c++)
				out += at(
					270 + c * 121,
					260 + r * 130,
					1,
					(r + c) % 2 ? 8 : -8,
					star(44, cols[(r * 5 + c) % 5])
				);
		out += putFrog(512, 640, 2.6, { arms: 'up', eyes: 'starry', mouth: 'grin' });
		return out;
	},
	'ratings.2500': () => {
		let out = glow(512, 330, 300, '#ffe68a', 0.3);
		out += at(512, 260, 1, 0, star(110, '#ffd25e', { edge: '#e0a91c' }));
		out += at(512, 168, 0.5, 0, crown());
		out += shadow(512, 744, 160, 16);
		out += putFrog(512, 520, 3.6, { arms: 'up', wear: ['monocle'], mouth: 'smirk' });
		return out;
	},

	// chapters -----------------------------------------------------------------------
	'chapters.1': () => {
		let out = glow(512, 470, 300, '#fff3c4', 0.22);
		out += shadow(512, 712, 200, 16);
		out += at(512, 560, 1.3, 0, openBook({ cover: '#3f7fe0' }));
		out += line(
			'M380 360 Q370 310 390 270 M512 340 L512 260 M640 360 Q654 310 634 270',
			'#fff3c4',
			10,
			{ opacity: 0.7 }
		);
		out += at(600, 500, 0.4, -20, tadpole());
		out += sparkles([
			[300, 300, 22],
			[720, 300, 26]
		]);
		return out;
	},
	'chapters.100': () => {
		let out = shadow(512, 742, 200, 16);
		out += at(512, 640, 1.1, 0, openBook({ cover: '#e4572e' }));
		out +=
			at(
				330,
				400,
				0.6,
				-30,
				path('M0 0 Q60 -30 120 0 L120 90 Q60 60 0 90 Z', C.paper, { stroke: '#cfc8b6', sw: 4 })
			) +
			at(
				700,
				360,
				0.5,
				30,
				path('M0 0 Q60 -30 120 0 L120 90 Q60 60 0 90 Z', C.paper, { stroke: '#cfc8b6', sw: 4 })
			);
		out += putFrog(530, 360, 3.6, { pose: 'leap', eyes: 'happy', mouth: 'open' }, -10);
		return out;
	},
	'chapters.500': () => {
		let out = glow(512, 450, 300, '#fff3c4', 0.2);
		for (let i = 0; i < 5; i++)
			out += at(
				512,
				620,
				1,
				-60 + i * 30,
				path('M0 0 Q-20 -120 0 -240 Q30 -120 0 0 Z', '#fff6dc', { stroke: '#cfc8b6', sw: 4 })
			);
		out += putFrog(512, 450, 3.4, { arms: 'hold', eyes: 'starry', mouth: 'o' });
		out += at(512, 620, 0.85, 0, openBook({ cover: '#8f5bd6' }));
		return out;
	},
	'chapters.1000': () => {
		let out = shadow(560, 742, 220, 16);
		out +=
			at(600, 680, 1, 0, bookSpine(300, 60, '#3f7fe0')) +
			at(590, 620, 1, -3, bookSpine(280, 60, C.red)) +
			at(604, 560, 1, 2, bookSpine(260, 60, '#8f5bd6'));
		const worm =
			line('M530 520 Q560 480 590 520 Q620 560 650 520 Q672 490 690 500', '#ff9fbf', 34) +
			circle(700, 494, 30, '#ff9fbf') +
			circle(690, 486, 16, 'none', { stroke: C.ink, sw: 4 }) +
			circle(718, 486, 14, 'none', { stroke: C.ink, sw: 4 }) +
			circle(692, 488, 5, C.ink) +
			circle(718, 488, 5, C.ink);
		out += worm;
		out += putFrog(300, 520, 3.2, { look: [3, 0], mouth: 'smirk', eyes: 'focus' });
		return out;
	},
	'chapters.2500': () => {
		let out = shadow(512, 744, 190, 16);
		const colours = [
			'#3f7fe0',
			C.red,
			'#8f5bd6',
			'#4ea85f',
			'#f2c444',
			'#35b7c9',
			'#ff8fb1',
			'#e4572e'
		];
		colours.forEach((c, i) => {
			out += at(
				512 + Math.sin(i * 0.9) * 22 + i * 4,
				700 - i * 52,
				1,
				(i % 2 ? 4 : -4) + i,
				bookSpine(260 - i * 10, 50, c)
			);
		});
		out += putFrog(560, 220, 2.2, { arms: 'up', eyes: 'happy', mouth: 'grin' });
		return out;
	},
	'chapters.5000': () => {
		let out = padBase(680, 240, { flower: true });
		out +=
			at(330, 340, 1, -18, bookSpine(150, 40, '#3f7fe0')) +
			at(700, 300, 1, 14, bookSpine(150, 40, C.red)) +
			at(760, 470, 1, -8, bookSpine(130, 36, '#8f5bd6')) +
			at(280, 480, 1, 10, bookSpine(130, 36, '#f2c444'));
		out += putFrog(512, 480, 3.2, { arms: 'hold', eyes: 'closed', mouth: 'small', wear: ['grad'] });
		out += at(512, 615, 0.6, 0, openBook({ cover: '#4ea85f' }));
		return out;
	},

	// manga --------------------------------------------------------------------------
	'manga.1': () => {
		let out = shadow(512, 742, 170, 16);
		out += rect(370, 260, 290, 430, 18, '#c94a26') + rect(390, 260, 270, 430, 14, '#f5f1e4');
		out += rect(390, 300, 270, 200, 0, '#23342b');
		out += at(525, 420, 2.4, 0, peek({ eyes: 'starry' }));
		out += line('M410 540 L640 540 M410 580 L600 580', '#c94a26', 10);
		out += at(
			640,
			620,
			0.9,
			0,
			circle(0, 0, 70, '#4ea85f') + at(0, 0, 0.5, 0, check('#ffffff', 40))
		);
		return out;
	},
	'manga.10': () => {
		let out =
			rect(230, 250, 564, 450, 18, '#8c5225') +
			rect(250, 270, 524, 410, 12, '#5e3416') +
			rect(250, 465, 524, 16, 4, '#8c5225');
		const cols = ['#e4572e', '#3f7fe0', '#f2c444', '#8f5bd6', '#35b7c9', '#ff8fb1', '#4ea85f'];
		for (let row = 0; row < 2; row++)
			for (let i = 0; i < 9; i++)
				out +=
					rect(
						266 + i * 56,
						290 + row * 200 - (i % 3) * 6,
						48,
						170 + (i % 3) * 6,
						6,
						cols[(i + row * 3) % 7]
					) + rect(276 + i * 56, 320 + row * 200, 28, 30, 4, '#ffffff', { opacity: 0.6 });
		out += at(720, 268, 1.5, 0, peek({ eyes: 'happy' }));
		return out;
	},
	'manga.25': () => {
		let out = shadow(512, 742, 200, 16);
		out += path('M260 640 Q400 560 520 620 Q640 680 780 600', 'none', {
			stroke: '#11231a',
			sw: 26
		});
		out += rect(660, 600, 110, 100, 20, '#23342b') + ellipse(715, 600, 55, 14, '#000000');
		out += putFrog(450, 440, 3.6, {
			arms: 'right',
			eyes: 'focus',
			mouth: 'small',
			wear: ['beret']
		});
		out += at(
			590,
			420,
			1,
			30,
			rect(-12, -100, 24, 170, 8, '#a8642e') + path('M-16 70 Q0 140 16 70 Z', '#11231a')
		);
		return out;
	},
	'manga.50': () => {
		let out = glow(512, 430, 300, '#fff3c4', 0.2);
		out += shadow(512, 744, 200, 16);
		const cols = ['#e4572e', '#3f7fe0', '#f2c444', '#8f5bd6', '#35b7c9'];
		cols.forEach(
			(c, i) =>
				(out += at(
					512,
					714 - i * 40,
					1,
					i % 2 ? 2 : -2,
					bookSpine(300 - i * 14, 38, c, { label: false })
				))
		);
		out += putFrog(512, 400, 3.3, {
			arms: 'hold',
			eyes: 'closed',
			mouth: 'small',
			wear: ['headband']
		});
		out += at(512, 505, 0.8, 90, rect(-12, -80, 24, 160, 8, '#a8642e'));
		return out;
	},

	// streaks ------------------------------------------------------------------------
	'streak.3': () => {
		let out = glow(512, 430, 260, '#ff9a3c', 0.3);
		out += shadow(512, 744, 160, 16);
		out += putFrog(512, 470, 3.8, { arms: 'hold', eyes: 'happy', mouth: 'small' });
		out += at(512, 560, 0.2, 0, flame({ id: 's3' }));
		return out;
	},
	'streak.7': () => {
		let out = glow(512, 500, 340, '#ff9a3c', 0.42);
		out += sparkles(
			[
				[690, 280, 12],
				[342, 330, 9],
				[730, 420, 7]
			],
			'#ffb14a'
		);
		out += ellipse(512, 744, 190, 22, '#ff9a3c', { opacity: 0.3 });
		out += at(512, 500, 0.95, 0, flame({ id: 's7', core: false }));
		out += at(
			512,
			590,
			2.6,
			0,
			frog({
				pose: 'head',
				eyes: 'happy',
				mouth: 'grin',
				colors: {
					skin: '#fff3c4',
					skinDeep: '#ffcf5a',
					belly: '#ffe9a8',
					blush: '#ff8a5a',
					pupil: '#b53a1c',
					mouth: '#b53a1c'
				}
			})
		);
		return out;
	},
	'streak.14': () => {
		let out = glow(512, 500, 340, '#ff9a3c', 0.38);
		out += line('M330 720 L700 660 M330 660 L700 720', '#8c5225', 40);
		out +=
			at(430, 520, 0.7, -8, flame({ id: 's14a' })) + at(600, 500, 0.8, 8, flame({ id: 's14b' }));
		return out;
	},
	'streak.30': () => {
		let out = glow(512, 480, 340, '#ff9a3c', 0.36);
		out += circle(512, 470, 260, 'none', { stroke: '#ffb14a', sw: 26 });
		out += circle(512, 470, 260, 'none', { stroke: '#fff3c4', sw: 6, opacity: 0.7 });
		for (let i = 0; i < 12; i++)
			out += at(
				512 + Math.cos((i * 30 * Math.PI) / 180) * 260,
				470 + Math.sin((i * 30 * Math.PI) / 180) * 260,
				0.12,
				i * 30 + 90,
				flame({ id: 's30r', core: false })
			);
		out += at(512, 500, 0.8, 0, flame({ id: 's30' }));
		return out;
	},
	'streak.100': () => {
		let out = sky('#14243d', '#2b2350');
		out += stars([
			[260, 260, 4],
			[760, 240, 5],
			[800, 600, 4],
			[300, 640, 3],
			[700, 690, 12]
		]);
		out += inDisc(
			path(
				'M690 300 Q420 420 140 760 Q480 520 720 360 Z',
				linear(
					'tail',
					[
						[0, '#ffb14a', 0.9],
						[1, '#ff6b5a', 0]
					],
					{ x1: 1, y1: 0, x2: 0, y2: 1 }
				)
			)
		);
		out += inDisc(
			path('M700 320 Q520 420 300 640 Q540 470 730 370 Z', '#fff3c4', { opacity: 0.6 })
		);
		out += circle(700, 330, 90, '#ffd25e') + circle(700, 330, 60, '#fff3c4');
		out += putFrog(690, 230, 2.2, { eyes: 'happy', mouth: 'open', arms: 'up' });
		return out;
	},
	'streak.365': () => {
		let out = glow(512, 300, 300, '#ff9a3c', 0.42);
		out += at(
			512,
			470,
			1,
			0,
			laurel(-1, 300, { color: '#f2c444', dark: '#c99a1e', stem: '#a8740c' }) +
				laurel(1, 300, { color: '#f2c444', dark: '#c99a1e', stem: '#a8740c' })
		);
		out += at(512, 260, 0.5, 0, flame({ id: 's365' }));
		out +=
			path('M440 380 L584 380 L560 470 L464 470 Z', C.gold) +
			rect(486, 470, 52, 150, 10, '#a8740c');
		out += putFrog(512, 520, 2.8, { arms: 'up', eyes: 'happy', mouth: 'grin' });
		return out;
	},

	// rewatches -----------------------------------------------------------------------
	'rewatches.1': () => {
		let out = glow(512, 470, 300, '#8af0b4', 0.2);
		out += at(512, 470, 1, 0, loopArrow(190, '#8af0b4'));
		out +=
			circle(512, 470, 120, '#23342b') +
			poly(
				[
					[480, 410],
					[570, 470],
					[480, 530]
				],
				'#ffffff'
			);
		return out;
	},
	'rewatches.10': () => {
		let out = glow(512, 480, 300, '#b48cff', 0.2);
		out += shadow(512, 744, 230, 16);
		out += group(putFrog(600, 470, 3.6, { eyes: 'wide', mouth: 'o' }), { opacity: 0.4 });
		out += putFrog(440, 490, 3.6, { eyes: 'wide', mouth: 'o' });
		out += line('M650 270 Q700 240 740 270', '#ffffff', 8, { opacity: 0.6 });
		return out;
	},
	'rewatches.50': () => {
		let out = glow(512, 460, 320, '#8af0b4', 0.2);
		out += circle(512, 450, 230, 'none', { stroke: '#23342b', sw: 80 });
		out += circle(512, 450, 230, 'none', { stroke: C.cream, sw: 14, opacity: 1 });
		out += `<circle cx="512" cy="450" r="230" fill="none" stroke="#23342b" stroke-width="10" stroke-dasharray="14 22"/>`;
		out += putFrog(512, 196, 2.4, { pose: 'leap', eyes: 'happy', mouth: 'open' }, 0);
		out += at(512, 450, 0.9, 0, loopArrow(90, '#8af0b4'));
		return out;
	},

	// extension ---------------------------------------------------------------------
	'extension.10': () => {
		let out = glow(512, 480, 300, '#8fb6ff', 0.24);
		out += at(380, 380, 2.5, -10, peek({ eyes: 'starry' }));
		out += at(530, 520, 1.15, 0, puzzle());
		return out;
	},
	'extension.100': () => {
		let out = glow(512, 460, 320, '#8ff0ff', 0.2);
		out += shadow(512, 744, 160, 16);
		out += putFrog(512, 450, 3.8, {
			colors: ROBO_FROG,
			arms: 'hold',
			wear: ['antenna'],
			eyes: 'starry',
			mouth: 'grin'
		});
		out += line('M440 520 L470 520 M554 520 L584 520', '#6f8396', 4);
		out += at(512, 600, 0.45, 0, puzzle());
		return out;
	},
	'extension.1000': () => {
		let out = glow(512, 420, 300, '#8ff0ff', 0.25);
		out += at(512, 390, 1, 0, screens());
		out += rect(240, 600, 544, 110, 22, '#3e4650');
		out += putFrog(512, 560, 2.6, {
			arms: 'hold',
			wear: ['headset'],
			eyes: 'focus',
			mouth: 'small'
		});
		return out;
	},

	// favourites --------------------------------------------------------------------
	'favourites.5': () => {
		let out = at(512, 680, 1, 0, pond(320, 70));
		out += at(
			512,
			640,
			2.7,
			0,
			path(
				'M0 38 C-12 28 -56 2 -56 -24 C-56 -48 -30 -60 0 -34 C30 -60 56 -48 56 -24 C56 2 12 28 0 38 Z',
				C.pad
			) + line('M0 -34 L0 30', '#ffffff', 2, { opacity: 0.2 })
		);
		out += putFrog(512, 500, 2.4, { eyes: 'hearts', mouth: 'grin' });
		return out;
	},
	'favourites.25': () => {
		let out = glow(512, 460, 300, '#ff8fb1', 0.28);
		const d =
			'M512 690 C470 650 280 540 280 400 C280 290 380 250 512 350 C644 250 744 290 744 400 C744 540 554 650 512 690 Z';
		out += line(d, '#23342b', 70) + line(d, C.cream, 12, { dash: '14 22' });
		out += at(512, 470, 2.6, 0, frog({ pose: 'head', eyes: 'hearts', mouth: 'grin' }));
		return out;
	},
	'favourites.100': () => {
		let out = padBase(690, 250, { flower: false });
		out += putFrog(410, 520, 3, { eyes: 'happy', mouth: 'grin', look: [2, 0] });
		out += putFrog(614, 520, 3, { eyes: 'happy', mouth: 'grin', wear: ['bow'], colors: PINK_FROG });
		out += at(512, 300, 1.3, 0, heart(C.red));
		out += at(420, 230, 0.5, -15, heart(C.pink)) + at(620, 220, 0.4, 15, heart(C.pink));
		return out;
	},

	// years ---------------------------------------------------------------------------
	'years.1': () => {
		let out = glow(512, 440, 300, '#ffe9a8', 0.24);
		out += at(590, 590, 1, 0, cake(1));
		out += putFrog(330, 500, 2.8, { arms: 'up', wear: ['party'], eyes: 'happy', mouth: 'grin' });
		out += confetti([
			[300, 300, C.yellow, 20],
			[720, 280, C.pink, -30],
			[760, 460, C.teal, 10],
			[270, 480, '#8af0b4', 40]
		]);
		return out;
	},
	'years.2': () => {
		let out = glow(512, 440, 300, '#9be27a', 0.24);
		out += at(512, 560, 1.15, 0, cake(2));
		out += at(270, 700, 0.9, -10, sprout()) + at(760, 700, 0.8, 12, sprout());
		out += confetti([
			[300, 300, C.yellow, 20],
			[720, 280, C.pink, -30]
		]);
		return out;
	},
	'years.5': () => {
		let out = glow(512, 430, 300, '#ffe9a8', 0.24);
		out += putFrog(512, 330, 2.8, {
			wear: ['monocle'],
			eyes: 'happy',
			mouth: 'smile',
			colors: { skin: '#7fb069', skinDeep: '#5c8c4c', belly: '#b5d99c' }
		});
		out += line('M440 340 Q420 260 460 250', '#ffffff', 8, { opacity: 0.5 });
		out += at(512, 590, 1.1, 0, cake(5));
		return out;
	},

	// together ------------------------------------------------------------------------
	'together.1': () => {
		const s = sofa({ color: '#8f5bd6', deep: '#6a3fb0', light: '#b48cff' });
		let out = glow(512, 300, 280, '#8ff0ff', 0.18);
		out += at(512, 620, 0.95, 0, s.back);
		out += putFrog(420, 450, 2.8, { arms: 'hold', eyes: 'starry', mouth: 'open', look: [1, -2] });
		out += putFrog(604, 450, 2.8, {
			arms: 'hold',
			eyes: 'starry',
			mouth: 'grin',
			colors: BLUE_FROG,
			look: [-1, -2]
		});
		out += at(512, 560, 0.16, 0, popcorn({ id: 'pt1', band: false }));
		out += at(512, 620, 0.95, 0, s.front);
		return out;
	},
	'together.25': () => {
		let out = padBase(690, 270);
		out += putFrog(340, 510, 2.6, { eyes: 'closed', mouth: 'open' });
		out += putFrog(512, 470, 2.9, { eyes: 'closed', mouth: 'open', colors: BLUE_FROG });
		out += putFrog(684, 510, 2.6, { eyes: 'closed', mouth: 'open', colors: PINK_FROG });
		out += note(300, 300, 1) + note(690, 260, 0.9, '#8ff0ff') + note(512, 220, 0.8, '#ffc4d8');
		return out;
	},
	'together.100': () => {
		const s = sofa({ color: '#e4572e', deep: '#b8411f', light: '#f07a50' });
		let out = glow(512, 300, 300, '#8ff0ff', 0.18);
		out += at(512, 620, 1.05, 0, s.back);
		const palette = [
			{},
			BLUE_FROG,
			PINK_FROG,
			{ skin: '#f2c444', skinDeep: '#c99a1e', belly: '#ffe68a' },
			{}
		];
		[
			[290, 480],
			[400, 450],
			[512, 470],
			[624, 450],
			[734, 480]
		].forEach(
			([x, y], i) =>
				(out += putFrog(x, y, 2.1, {
					arms: 'hold',
					eyes: i % 2 ? 'starry' : 'happy',
					mouth: i % 2 ? 'open' : 'grin',
					colors: palette[i]
				}))
		);
		out += at(512, 620, 1.05, 0, s.front);
		return out;
	},

	// moments ------------------------------------------------------------------------
	'double-feature': () => {
		let out = glow(512, 500, 320, '#ffe9a8', 0.24);
		out += at(450, 470, 1.25, -16, ticket(300, 150, { color: '#ffd25e' }));
		out += at(
			580,
			560,
			1.25,
			10,
			ticket(300, 150, { color: '#ff8fb1', ink: '#a8305a', star: false }) +
				at(-40, 0, 1.2, 0, peek({ colors: {} }))
		);
		return out;
	},
	'triple-bill': () => {
		let out = glow(512, 520, 320, '#ffe9a8', 0.24);
		for (const [r, c] of [
			[-28, '#8ff0ff'],
			[0, '#ff8fb1'],
			[28, '#ffd25e']
		])
			out += at(512, 720, 1, r, at(0, -250, 1.15, 90, ticket(300, 140, { color: c })));
		out += circle(512, 720, 26, C.gold);
		return out;
	},
	'film-festival': () => {
		let out = glow(512, 500, 340, '#ffe68a', 0.3);
		out += at(
			512,
			520,
			1,
			0,
			laurel(-1, 320, { color: '#f2c444', dark: '#c99a1e', stem: '#a8740c', leaves: 8 }) +
				laurel(1, 320, { color: '#f2c444', dark: '#c99a1e', stem: '#a8740c', leaves: 8 })
		);
		out += at(512, 500, 1, 0, reel(150));
		for (let i = 0; i < 5; i++)
			out += at(352 + i * 80, 260 - Math.sin((i / 4) * Math.PI) * 40, 1, 0, star(32, C.yellow));
		out += at(512, 760, 1, 0, rect(-150, -30, 300, 60, 20, '#c62a3c'));
		return out;
	},
	marathon: () => {
		let out = glow(512, 500, 320, '#8fb6ff', 0.2);
		out += at(512, 640, 1, -6, filmStrip(640, 110, { frames: 6 }));
		out += at(512, 500, 1.35, -8, shoe('#3f7fe0'));
		out += motion(300, 440, 3, 90, 30);
		out += at(640, 360, 1.6, 0, peek({ eyes: 'happy' }));
		return out;
	},
	ultramarathon: () => {
		let out = glow(512, 460, 330, '#b48cff', 0.24);
		out += shadow(512, 812, 170, 18);
		out += putFrog(512, 470, 4.6, {
			arms: 'up',
			eyes: 'happy',
			mouth: 'grin',
			wear: ['headband', 'medal']
		});
		out += confetti([
			[280, 300, C.yellow, 20],
			[740, 280, C.pink, -30],
			[780, 470, C.teal, 10],
			[250, 480, '#8af0b4', 40],
			[700, 680, C.yellow, -20],
			[320, 690, C.pink, 15]
		]);
		return out;
	},
	'one-sitting': () => {
		const s = sofa({ color: '#4ea85f', deep: '#2f7d46', light: '#7acd79' });
		let out = glow(512, 360, 300, '#8af0b4', 0.2);
		out += at(512, 640, 1.1, 0, s.back);
		out += at(512, 640, 1.1, 0, s.front);
		out += at(512, 650, 0.65, 0, check('#ffffff', 40));
		out += at(
			512,
			330,
			0.55,
			0,
			tv(380, 280, {
				screen: poly(
					[
						[-30, -40],
						[40, 0],
						[-30, 40]
					],
					'#ffffff'
				)
			})
		);
		return out;
	},
	speedrun: () => {
		let out = glow(512, 480, 320, '#ff9a8a', 0.2);
		out += at(512, 640, 0.8, 0, tv(380, 280, { antenna: false }));
		out += at(512, 390, 1, 0, stopwatch(130));
		out += motion(330, 300, 3, 70, 30);
		return out;
	},
	metamorphosis: () => {
		let out = glow(512, 500, 340, '#8af0b4', 0.24);
		out +=
			path('M330 300 A240 240 0 0 1 760 420', 'none', { stroke: '#8af0b4', sw: 18 }) +
			poly(
				[
					[780, 470],
					[730, 404],
					[800, 396]
				],
				'#8af0b4'
			);
		out +=
			path('M694 720 A240 240 0 0 1 268 600', 'none', { stroke: '#8af0b4', sw: 18 }) +
			poly(
				[
					[244, 550],
					[300, 612],
					[232, 624]
				],
				'#8af0b4'
			);
		out += at(330, 420, 1.1, -12, tadpole());
		out += putFrog(650, 560, 3.4, { arms: 'up', eyes: 'starry', mouth: 'grin' });
		out += at(400, 640, 0.3, 0, tv(380, 280, { antenna: false, legs: false }));
		return out;
	},
	'fly-feast': () => {
		let out = glow(512, 480, 340, '#d6ff8a', 0.2);
		out += shadow(400, 812, 170, 18);
		out += line('M450 560 Q600 480 700 330', C.pink, 18);
		out += at(705, 322, 1.6, -30, fly());
		for (const [x, y, s, r] of [
			[300, 260, 1.3, 20],
			[610, 220, 1.2, -10],
			[780, 470, 1.3, 30],
			[250, 520, 1.1, -20],
			[520, 300, 1, 10],
			[790, 640, 1, -30]
		])
			out += at(x, y, s, r, fly());
		out += putFrog(400, 560, 4.4, { eyes: 'focus', mouth: 'open', look: [4, -3] });
		return out;
	},
	'all-nighter': () => {
		let out = sky('#0f1a33', '#162a48');
		out += stars([
			[250, 250, 5],
			[330, 190, 3],
			[780, 280, 4],
			[700, 200, 13]
		]);
		out += at(300, 300, 1, -20, moon(60));
		out += inDisc(
			poly(
				[
					[560, 500],
					[960, 300],
					[960, 900],
					[560, 640]
				],
				radial(
					'tvglow',
					[
						[0, '#5fd3ff', 0.5],
						[1, '#5fd3ff', 0]
					],
					{ cx: 0, cy: 0.5, r: 1 }
				)
			)
		);
		out += at(760, 600, 0.5, 0, tv(380, 280, { antenna: false }));
		out += putFrog(430, 560, 4, {
			arms: 'hold',
			eyes: 'wide',
			mouth: 'small',
			wear: ['nightcap'],
			colors: { skin: '#5fbf9a', skinDeep: '#3f9a7a', belly: '#8ad8c0' }
		});
		out += at(430, 740, 0.24, 0, popcorn({ id: 'pan', band: false }));
		return out;
	},
	'croak-around-the-clock': () => {
		let out = glow(512, 512, 340, '#ffe9a8', 0.24);
		out += at(512, 512, 1, 0, clock(320, { hands: false, face: '#fff6dc' }));
		for (let i = 0; i < 24; i++) {
			const a = ((i * 15 - 90) * Math.PI) / 180;
			out += circle(
				512 + Math.cos(a) * 252,
				512 + Math.sin(a) * 252,
				i % 6 === 0 ? 14 : 8,
				i < 12 ? '#ffb14a' : '#3f7fe0'
			);
		}
		out += line('M512 512 L512 300', C.red, 16) + line('M512 512 L650 600', '#23342b', 14);
		out += at(512, 512, 3.2, 0, frog({ pose: 'head', eyes: 'happy', mouth: 'open' }));
		return out;
	},
	'weekend-warrior': () => {
		const s = sofa({ color: '#f2c444', deep: '#c99a1e', light: '#ffe07a' });
		let out = glow(512, 360, 300, '#ffe9a8', 0.26);
		out += at(300, 260, 1, 0, sun(46));
		out += at(512, 660, 1.1, 0, s.back);
		out += putFrog(512, 480, 3.8, {
			arms: 'up',
			eyes: 'happy',
			mouth: 'grin',
			wear: ['sunglasses']
		});
		out += at(512, 660, 1.1, 0, s.front);
		return out;
	},
	'day-one': () => {
		let out = glow(512, 500, 320, '#ffe68a', 0.24);
		out += shadow(512, 812, 180, 18);
		out += at(
			512,
			520,
			1.15,
			0,
			calendar(340, 380, {
				inside: poly(
					[
						[30, -110],
						[-50, 20],
						[0, 20],
						[-30, 120],
						[60, -20],
						[8, -20],
						[40, -110]
					],
					C.yellow,
					{ stroke: '#e0a91c', sw: 6 }
				)
			})
		);
		out += at(690, 300, 1.8, 20, peek({ eyes: 'starry' }));
		return out;
	},
	'live-tonight': () => {
		let out = glow(512, 440, 340, '#ff6b5a', 0.32);
		for (const r of [200, 260, 320])
			out +=
				path(
					`M${512 - r * 0.7} ${440 - r * 0.7} A${r} ${r} 0 0 0 ${512 - r * 0.7} ${440 + r * 0.7}`,
					'none',
					{ stroke: '#ff8a7a', sw: 12, opacity: 1.2 - r / 400 }
				) +
				path(
					`M${512 + r * 0.7} ${440 - r * 0.7} A${r} ${r} 0 0 1 ${512 + r * 0.7} ${440 + r * 0.7}`,
					'none',
					{ stroke: '#ff8a7a', sw: 12, opacity: 1.2 - r / 400 }
				);
		out +=
			rect(352, 340, 320, 200, 40, '#5a1414') +
			rect(
				368,
				356,
				288,
				168,
				30,
				radial('onair', [
					[0, '#ff8a7a'],
					[1, '#d9302a']
				])
			);
		out += at(
			512,
			440,
			2.2,
			0,
			frog({
				pose: 'head',
				eyes: 'starry',
				mouth: 'open',
				colors: { skin: '#fff0ec', skinDeep: '#ffd0c8', belly: '#ffffff', blush: '#ff8a7a' }
			})
		);
		out += line('M512 540 L512 660 M430 660 L594 660', '#9aa6a0', 16);
		return out;
	},
	'season-ticket': () => {
		let out = glow(512, 500, 320, '#b48cff', 0.24);
		out += at(512, 512, 1, 0, loopArrow(260, '#d2adff'));
		out += at(512, 512, 1.45, -10, ticket(320, 150, { color: '#ffd25e' }));
		out += at(470, 512, 1.4, -10, peek({ eyes: 'happy' }));
		return out;
	},
	'premiere-night': () => {
		let out = at(512, 120, 1, 0, spotlight(0, 640, 250, { id: 'pn' }));
		out += at(512, 860, 1, 0, carpet(560, 300));
		out +=
			line('M240 700 L240 520 M784 700 L784 520', C.gold, 14) +
			path('M240 540 Q512 640 784 540', 'none', { stroke: '#c62a3c', sw: 18 });
		out += putFrog(512, 540, 3.9, { arms: 'wave', wear: ['bow'], eyes: 'starry', mouth: 'grin' });
		out += sparkles([
			[250, 330, 30],
			[780, 360, 26]
		]);
		return out;
	},
	'early-riser': () => {
		let out = inDisc(
			rect(
				60,
				60,
				904,
				904,
				0,
				linear('dawn', [
					[0, '#2b4a7a'],
					[0.55, '#ff9a6a'],
					[1, '#ffd29a']
				])
			)
		);
		out += at(512, 640, 1, 0, sun(110, '#fff3c4'));
		out += at(512, 760, 1, 0, pond(460, 160));
		out += at(512, 700, 1, 0, lilyPad(240, 62));
		out += putFrog(512, 500, 3.8, { arms: 'right', eyes: 'happy', mouth: 'grin' });
		out += at(650, 470, 0.8, -20, ticket(200, 96, { color: '#ffd25e' }));
		return out;
	},
	patience: () => {
		let out = glow(512, 500, 320, '#cfe9ff', 0.16);
		out += shadow(512, 812, 300, 18);
		out += putFrog(330, 560, 3.4, { eyes: 'sleepy', mouth: 'flat', arms: 'hold' });
		out += at(640, 700, 1, 0, snail({ ticket: true }));
		out += zed(380, 300, 40) + zed(430, 250, 30);
		return out;
	},
	'spooky-season': () => {
		let out = sky('#1b1030', '#2b2350');
		out += at(700, 270, 1, 0, circle(0, 0, 90, '#fff3c4'));
		out += glow(700, 270, 200, '#fff3c4', 0.2);
		for (const [x, y, s] of [
			[320, 260, 1],
			[420, 210, 0.7],
			[580, 320, 0.8]
		])
			out += at(
				x,
				y,
				s,
				0,
				path(
					'M0 0 Q-20 -20 -50 -10 Q-34 -4 -36 10 Q-20 0 0 10 Q20 0 36 10 Q34 -4 50 -10 Q20 -20 0 0 Z',
					'#0d0818'
				)
			);
		out += at(512, 620, 1.25, 0, pumpkin());
		out += at(300, 560, 1.7, 0, peek({ eyes: 'wide' })) + at(300, 640, 0.7, 0, cushion('#8f5bd6'));
		return out;
	},
	'summer-blockbuster': () => {
		let out = glow(512, 480, 340, '#ffd25e', 0.3);
		out += at(512, 250, 1, 0, sun(70));
		out += at(512, 520, 2.6, 0, frog({ pose: 'head', mouth: 'grin', eyes: 'happy' }));
		out += at(384, 470, 1, 0, reel(80)) + at(640, 470, 1, 0, reel(80));
		out +=
			line('M464 470 Q512 450 560 470', '#23342b', 14) +
			line('M304 460 L240 420 M720 460 L784 420', '#23342b', 14);
		out += at(512, 780, 1, 0, path('M-260 0 Q0 -60 260 0 Z', '#f2d4a6'));
		return out;
	},
	fireworks: () => {
		let out = sky('#0f1a33', '#1d3a4f');
		out +=
			at(340, 300, 1, 0, burst(120, '#ff8fb1')) +
			at(680, 260, 1, 0, burst(140, '#ffd25e')) +
			at(560, 420, 1, 0, burst(80, '#8ff0ff')) +
			at(260, 500, 1, 0, burst(60, '#8af0b4'));
		out += at(512, 780, 1, 0, pond(460, 120));
		out += at(512, 740, 1, 0, lilyPad(200, 54));
		out += putFrog(512, 590, 2.9, { arms: 'up', eyes: 'starry', mouth: 'open', look: [0, -3] });
		return out;
	},
	'leap-year': () => {
		let out = glow(512, 520, 340, '#8af0b4', 0.24);
		out += shadow(512, 812, 220, 18);
		const inside = label('29', 0, 0, 120, '#23342b', 20).svg;
		out += at(420, 620, 1, -4, calendar(280, 300, { inside }));
		out += line('M240 480 Q420 120 700 300', '#ffffff', 8, { dash: '4 22', opacity: 0.7 });
		out += putFrog(680, 330, 3.6, { pose: 'leap', eyes: 'happy', mouth: 'open' }, -16);
		out += sparkles([
			[300, 300, 28],
			[800, 520, 22]
		]);
		return out;
	},
	'back-from-the-dead': () => {
		let out = glow(512, 480, 340, '#c9a8ff', 0.26);
		out += shadow(512, 812, 260, 18);
		// A dusty old television, the show it was left on, switched back on.
		out += at(
			600,
			590,
			1.18,
			6,
			tv(380, 290, { screen: at(0, 0, 1.9, 0, peek({ eyes: 'happy' })) })
		);
		out += at(
			300,
			640,
			1,
			-8,
			path('M-80 150 L-80 -40 Q-80 -130 0 -130 Q80 -130 80 -40 L80 150 Z', '#9fb3a6')
		);
		out += at(
			300,
			640,
			1,
			-8,
			path('M-62 150 L-62 -34 Q-62 -110 0 -110 Q62 -110 62 -34 L62 150 Z', '#b8c8bd')
		);
		out += at(300, 640, 1, -8, line('M-30 -50 L30 -50 M0 -80 L0 -10', '#7f948a', 12));
		out += at(300, 640, 1, -8, line('M-20 40 L4 70 L-12 96', '#7f948a', 6));
		out += at(330, 330, 0.9, -20, loopArrow(70, '#c9a8ff'));
		out += sparkles([
			[760, 320, 22],
			[820, 420, 14],
			[440, 250, 16]
		]);
		return out;
	},
	'watchlist-zero': () => {
		let out = glow(512, 480, 340, '#8af0b4', 0.26);
		out += shadow(512, 830, 230, 18);
		// A clipboard, every line ticked off.
		out += at(512, 560, 1, -4, rect(-190, -250, 380, 500, 30, '#8a5a2b'));
		out += at(512, 560, 1, -4, rect(-160, -210, 320, 440, 14, C.paper));
		out += at(512, 560, 1, -4, rect(-70, -276, 140, 60, 18, '#9fb3a6'));
		for (const y of [-130, -40, 50, 140])
			out += at(
				512,
				560,
				1,
				-4,
				at(-90, y, 0.42, 0, check('#4ea85f')) + line(`M-30 ${y} L120 ${y}`, '#b9b2a0', 12)
			);
		out += putFrog(700, 360, 2.4, { eyes: 'happy', mouth: 'open', arms: 'up' }, 8);
		out += sparkles([
			[300, 300, 20],
			[250, 420, 12],
			[790, 600, 16]
		]);
		return out;
	},
	'spring-thaw': () => {
		let out = glow(512, 360, 320, '#fff3c4', 0.3);
		out += at(700, 260, 1, 0, sun(60, '#ffd25e'));
		out += putFrog(512, 520, 4, {
			eyes: 'happy',
			mouth: 'grin',
			arms: 'up',
			wear: ['snowcap', 'sprout']
		});
		out += inDisc(at(512, 790, 1, 0, snow(900)));
		out += at(330, 740, 0.7, -10, sprout()) + at(700, 750, 0.6, 14, sprout());
		out += at(290, 640, 0.8, 0, blossom(34)) + at(750, 640, 0.7, 0, blossom(30));
		return out;
	},

	// totals on their own -------------------------------------------------------------
	'long-haul': () => {
		let out = glow(512, 480, 330, '#8fb6ff', 0.2);
		out += at(512, 580, 1.2, 0, road());
		out += putFrog(340, 680, 1.8, { eyes: 'focus', mouth: 'smile', look: [2, -3] });
		return out;
	},
	'grand-voyage': () => {
		let out = glow(512, 480, 340, '#ffe68a', 0.24);
		out += at(512, 500, 1, 0, shipWheel(220));
		out += at(512, 500, 1, -30, filmStrip(560, 70, { frames: 6 }));
		out += at(
			512,
			500,
			2.8,
			0,
			frog({ pose: 'head', wear: ['captain'], eyes: 'focus', mouth: 'grin' })
		);
		return out;
	},
	'caught-up': () => {
		let out = glow(512, 500, 320, '#8af0b4', 0.24);
		out += shadow(512, 812, 160, 16);
		out += at(512, 500, 1, 0, clock(250, { hands: false }));
		out += line('M440 520 L500 580 L620 400', '#2f7d46', 30);
		out += circle(512, 500, 16, C.red);
		out += at(700, 300, 1.6, 20, peek({ eyes: 'happy' }));
		return out;
	},
	'perfect-ten': () => {
		let out = glow(512, 480, 340, '#ffe68a', 0.32);
		out += ellipse(512, 250, 150, 34, 'none', { stroke: '#fff3b8', sw: 18 });
		out += at(512, 530, 1, 0, star(260, '#ffd25e', { edge: '#e0a91c' }));
		out += at(
			512,
			560,
			3,
			0,
			frog({
				pose: 'head',
				eyes: 'starry',
				mouth: 'grin',
				colors: { skin: '#ffe68a', skinDeep: '#e0a91c', belly: '#fff3c4', blush: '#ffb14a' }
			})
		);
		out += sparkles([
			[250, 330, 30],
			[780, 340, 26],
			[800, 640, 20]
		]);
		return out;
	},
	'deck-clearer': () => {
		let out = glow(512, 480, 320, '#8af0b4', 0.2);
		out += at(512, 520, 1.25, 0, cards());
		out += at(720, 300, 1.6, 20, peek({ eyes: 'happy' }));
		return out;
	},
	'tough-crowd': () => {
		let out = glow(512, 480, 320, '#cfe9ff', 0.16);
		out += shadow(512, 812, 180, 16);
		out += putFrog(512, 470, 4.4, { eyes: 'focus', mouth: 'flat', arms: 'hold' });
		out += line('M560 302 Q600 270 636 284', C.ink, 12);
		out += at(512, 640, 1, 0, star(46, '#9aa6a0'));
		return out;
	},
	'easy-to-please': () => {
		let out = glow(512, 500, 340, '#ffe68a', 0.3);
		out += at(512, 520, 1, 0, star(270, '#ffd25e', { edge: '#e0a91c' }));
		out +=
			circle(430, 470, 34, C.ink) +
			circle(594, 470, 34, C.ink) +
			circle(420, 458, 12, '#ffffff') +
			circle(584, 458, 12, '#ffffff');
		out +=
			path('M380 560 Q512 700 644 560 Q512 600 380 560 Z', '#7a3e10') +
			ellipse(512, 640, 50, 22, C.pink);
		out += at(512, 360, 1.5, 0, peek({ eyes: 'happy' }));
		return out;
	},
	'hot-take': () => {
		let out = glow(512, 500, 330, '#ff6b5a', 0.32);
		out += at(512, 520, 1.6, -18, chili());
		out +=
			at(700, 340, 0.24, 20, flame({ id: 'hot' })) + at(330, 330, 0.16, -20, flame({ id: 'hot' }));
		out += at(420, 470, 1.4, 0, peek({ eyes: 'wide' }));
		return out;
	},
	'hidden-gem': () => {
		let out = glow(470, 480, 300, '#8ff0ff', 0.28);
		out += at(470, 540, 1.3, 0, gem('#8ff0ff'));
		out += at(560, 440, 1, 0, magnifier(150, { clear: true }));
		out += putFrog(300, 700, 1.8, { eyes: 'starry', mouth: 'o', look: [3, -3] });
		return out;
	},
	'genre-hopper': () => {
		let out = at(512, 700, 1, 0, pond(420, 110));
		const pads = [
			[230, 660, '#e4572e'],
			[380, 600, '#f2c444'],
			[530, 640, '#4ea85f'],
			[680, 600, '#35b7c9'],
			[800, 670, '#8f5bd6']
		];
		for (const [x, y, c] of pads)
			out += at(x, y, 1, 0, lilyPad(70, 22, { color: c, deep: '#000000' }));
		out += line('M230 640 Q300 420 380 590 Q450 380 530 620', '#ffffff', 6, {
			dash: '4 18',
			opacity: 0.6
		});
		out += putFrog(640, 400, 3.2, { pose: 'leap', eyes: 'happy', mouth: 'open' }, -10);
		return out;
	},
	'world-tour': () => {
		let out = glow(512, 540, 340, '#8ff0ff', 0.26);
		out += shadow(512, 838, 210, 18);
		const ocean = radial('globe', [
			[0, '#5fc3e8'],
			[0.75, '#2c8ec9'],
			[1, '#1d6a99']
		]);
		out += circle(512, 590, 236, ocean);
		out +=
			'<clipPath id="globe-clip"><circle cx="512" cy="590" r="236"/></clipPath>' +
			'<g clip-path="url(#globe-clip)">' +
			path(
				'M318 470 Q380 404 462 436 Q506 488 452 530 Q424 610 360 600 Q296 548 318 470 Z',
				C.pad
			) +
			path(
				'M566 420 Q656 402 712 474 Q728 548 664 566 Q604 528 572 566 Q520 502 566 420 Z',
				C.pad
			) +
			path('M520 648 Q604 626 646 696 Q606 790 540 780 Q498 708 520 648 Z', C.pad) +
			ellipse(512, 590, 112, 236, 'none', { stroke: '#ffffff', sw: 6, opacity: 0.28 }) +
			line('M276 590 L748 590 M296 486 L728 486 M296 694 L728 694', '#ffffff', 6, {
				opacity: 0.28
			}) +
			'</g>';
		out += circle(512, 590, 236, 'none', { stroke: '#1d5a80', sw: 10 });
		out += line('M196 520 Q170 300 420 250 Q640 210 800 330', '#ffffff', 7, {
			dash: '4 20',
			opacity: 0.7
		});
		out += at(816, 352, 0.9, 30, plane());
		out += at(300, 690, 1, -12, flag(170, { color: C.red }));
		out += at(744, 700, 1, 10, flag(150, { color: C.yellow }));
		out += putFrog(512, 350, 2.8, { eyes: 'happy', mouth: 'open' });
		return out;
	},
	'subtitles-on': () => {
		let out = glow(512, 500, 340, '#8ff0ff', 0.24);
		out += shadow(512, 812, 250, 18);
		const screen =
			at(0, 0, 2.1, 0, peek({ eyes: 'focus', look: [0, 2] })) +
			rect(-118, 42, 236, 70, 14, '#000000', { opacity: 0.62 }) +
			line('M-92 64 L92 64 M-62 92 L62 92', '#ffffff', 11);
		out += at(512, 560, 1.32, 0, tv(400, 300, { screen }));
		out += at(270, 330, 1, -10, rect(-58, -34, 116, 68, 20, C.paper));
		out += at(270, 330, 1, -10, line('M-30 -6 L30 -6 M-30 14 L10 14', '#b9b2a0', 9));
		out += path('M246 360 L232 400 L282 366 Z', C.paper);
		return out;
	},
	'brave-frog': () => {
		let out = glow(512, 480, 330, '#8f5bd6', 0.24);
		out += at(512, 452, 2.6, 0, peek({ eyes: 'wide', look: [2, 0] }));
		out += at(512, 650, 1.45, 0, cushion('#ff8fb1'));
		out += circle(430, 498, 20, C.skin) + circle(594, 498, 20, C.skin);
		out += line('M380 410 L350 380 M640 410 L670 380', '#ffffff', 8, { opacity: 0.6 });
		return out;
	},
	'laugh-track': () => {
		let out = glow(512, 500, 330, '#ffe68a', 0.3);
		out += at(512, 520, 1.6, -8, mask(C.yellow));
		out +=
			at(330, 300, 1, 0, line('M0 0 L30 -30 M20 20 L60 0', '#fff3c4', 10)) +
			at(690, 300, 1, 0, line('M0 0 L-30 -30 M-20 20 L-60 0', '#fff3c4', 10));
		out += at(512, 360, 1.3, 0, peek({ eyes: 'happy' }));
		return out;
	},
	documentarian: () => {
		let out = glow(512, 480, 320, '#cfe9ff', 0.18);
		out += shadow(512, 812, 220, 16);
		out += at(400, 560, 1.1, 0, mic());
		out += at(
			650,
			600,
			1,
			8,
			rect(-90, -130, 180, 260, 12, C.paper) +
				line('M-60 -70 L60 -70 M-60 -30 L60 -30 M-60 10 L40 10 M-60 50 L60 50', '#b9b2a0', 8) +
				rect(-90, -150, 180, 30, 8, C.red)
		);
		out += at(650, 330, 1.6, 0, peek({ eyes: 'focus' }));
		return out;
	},
	'drawn-in': () => {
		let out = glow(512, 480, 330, '#ffe9a8', 0.2);
		out += rect(220, 230, 584, 480, 20, C.paper);
		out += path('M260 640 Q330 320 400 640 Q450 420 500 640 Q540 500 580 640', 'none', {
			stroke: '#cfc8b6',
			sw: 8,
			opacity: 1
		});
		out += line('M260 640 L760 640', '#b9b2a0', 6);
		out += circle(580, 600, 40, C.red);
		out += at(690, 520, 1, -50, pencil());
		out += at(
			370,
			420,
			2.2,
			0,
			ellipse(0, 0, 25, 20, 'none', { stroke: '#7a7466', sw: 2.6 }) +
				circle(-13, -18, 11, C.paper, { stroke: '#7a7466', sw: 2.6 }) +
				circle(13, -18, 11, C.paper, { stroke: '#7a7466', sw: 2.6 }) +
				circle(-12, -17, 3.6, '#7a7466') +
				circle(12, -17, 3.6, '#7a7466') +
				line('M-9 3.5 Q0 12.5 9 3.5', '#7a7466', 2.6)
		);
		return out;
	},
	'time-traveller': () => {
		let out = glow(512, 480, 330, '#ffe9a8', 0.24);
		out += shadow(512, 812, 160, 16);
		out += at(512, 500, 1.55, 0, hourglass());
		out += at(512, 500, 1.6, 0, peek({ eyes: 'starry' }));
		return out;
	},
	'century-leap': () => {
		let out = glow(512, 500, 340, '#b48cff', 0.24);
		const tones = ['#5e5e5e', '#8a7a64', '#b5651d', '#e4572e', '#3f7fe0', '#8f5bd6'];
		tones.forEach(
			(c, i) =>
				(out += at(
					230 + i * 112,
					680 - (i % 2) * 20,
					0.85,
					i % 2 ? 6 : -6,
					poster(90, 130, c, { frame: i < 2 ? '#d8d2c0' : '#f5f1e4' })
				))
		);
		out += line('M220 520 Q512 120 820 480', '#ffffff', 8, { dash: '4 22', opacity: 0.6 });
		out += putFrog(560, 330, 3.6, { pose: 'leap', eyes: 'starry', mouth: 'open' }, -10);
		return out;
	},
	'silver-screen': () => {
		let out = glow(512, 480, 330, '#ffffff', 0.18);
		out += at(
			512,
			470,
			1,
			0,
			reel(230, {
				color: radial('bw', [
					[0, '#f0f0f0'],
					[1, '#9a9a9a']
				]),
				hole: '#2a2a2a'
			})
		);
		out += at(
			512,
			740,
			1,
			0,
			filmStrip(560, 100, { color: '#2a2a2a', holes: '#e0e0e0', frame: '#6a6a6a', frames: 5 })
		);
		out += at(
			512,
			470,
			2.4,
			0,
			frog({ pose: 'head', colors: GREY_FROG, mouth: 'smile', eyes: 'open' })
		);
		return out;
	},
	'directors-cut': () => {
		let out = glow(512, 480, 330, '#ffe9a8', 0.24);
		out += shadow(512, 812, 200, 16);
		out += at(512, 560, 1.25, 0, directorChair());
		out += putFrog(690, 330, 2.2, {
			wear: ['beret'],
			eyes: 'focus',
			mouth: 'smirk',
			arms: 'right'
		});
		return out;
	},
	'fan-club': () => {
		let out = glow(512, 480, 330, '#ffe68a', 0.3);
		out += at(512, 480, 1, -6, star(250, '#ffd25e', { edge: '#e0a91c' }));
		out += path(
			'M360 520 Q400 440 440 520 Q470 580 500 480 Q520 420 560 520 Q590 580 640 470',
			'none',
			{ stroke: '#7a3e10', sw: 14 }
		);
		out += at(700, 640, 1, -40, pencil());
		out += at(300, 680, 1.4, 0, peek({ eyes: 'hearts' }));
		return out;
	},
	'studio-loyal': () => {
		let out = glow(512, 480, 330, '#8fb6ff', 0.2);
		out += shadow(512, 780, 220, 16);
		out += at(512, 560, 1.25, 0, studio());
		out += at(512, 690, 0.9, 0, peek({}));
		return out;
	},
	'comfort-show': () => {
		let out = glow(512, 480, 330, '#ffe9a8', 0.24);
		out += shadow(512, 812, 200, 16);
		out += at(500, 540, 1.7, 0, peek({ eyes: 'closed' }));
		out += at(500, 620, 1.3, 0, mug('#4ea85f'));
		out += line('M440 420 Q400 360 450 320 Q500 280 460 220', '#ffffff', 14, { opacity: 0.5 });
		out += line('M560 420 Q600 360 550 320 Q500 280 540 220', '#ffffff', 14, { opacity: 0.5 });
		out += poly(
			[
				[460, 250],
				[580, 320],
				[460, 390]
			],
			'#ffffff',
			{ opacity: 0.85 }
		);

		return out;
	},
	curator: () => {
		let out = glow(512, 480, 330, '#ffe9a8', 0.2);
		out += shadow(512, 812, 220, 16);
		out += at(512, 560, 1.15, 0, shelf());
		out += putFrog(690, 270, 2.2, { wear: ['monocle'], arms: 'right', mouth: 'smirk' });
		return out;
	},
	'taste-twin': () => {
		let out = padBase(740, 280);
		out += putFrog(410, 520, 3.5, { eyes: 'closed', mouth: 'small', arms: 'down' });
		out += putFrog(614, 520, 3.5, { eyes: 'closed', mouth: 'small', colors: BLUE_FROG });
		out += at(512, 290, 0.9, 0, heart(C.pink));
		return out;
	},
	opposites: () => {
		let out = glow(512, 480, 330, '#8fb6ff', 0.2);
		out += shadow(512, 812, 300, 16);
		out += at(512, 330, 1.2, 0, apart());
		out += putFrog(330, 560, 3, { eyes: 'focus', mouth: 'flat', look: [-3, 0] });
		out += putFrog(694, 560, 3, { eyes: 'focus', mouth: 'flat', look: [3, 0], colors: BLUE_FROG });
		return out;
	},
	pioneer: () => {
		let out = padBase(740, 260);
		out += at(
			620,
			740,
			1,
			0,
			flag(420, { color: '#e4572e', mark: at(0, 0, 1.3, 0, peek({ colors: {} })) })
		);
		out += putFrog(400, 560, 3.4, { arms: 'right', eyes: 'happy', mouth: 'grin' });
		return out;
	},
	'king-of-the-pond': () => {
		let out = glow(512, 470, 340, '#ffe68a', 0.3);
		const rings = ['#d08c4e', '#c3cad2', '#f2c444', '#3fbf86', '#e2405a', '#4b86e6', '#a26de6'];
		for (let i = 0; i < 7; i++) {
			const a = ((i * (360 / 7) - 90) * Math.PI) / 180;
			const x = 512 + Math.cos(a) * 300;
			const y = 470 + Math.sin(a) * 260;
			out +=
				circle(x, y, 44, rings[i]) +
				circle(x, y, 32, '#18392a') +
				at(x, y, 1, 0, star(16, '#fff3c4'));
		}
		out += at(512, 700, 1, 0, lilyPad(200, 52, { flower: true }));
		out += putFrog(512, 520, 3.6, { arms: 'up', wear: ['crown'], eyes: 'starry', mouth: 'grin' });
		return out;
	},

	// anime ---------------------------------------------------------------------------
	'anime.1': () => {
		let out = glow(512, 440, 300, '#ffc4d8', 0.3);
		out += padBase(690, 220);
		out += putFrog(512, 500, 3.6, { eyes: 'starry', mouth: 'w', wear: ['bow'], arms: 'wave' });
		out += at(
			700,
			300,
			1,
			0,
			path(
				'M-80 -50 Q-80 -90 -40 -90 L60 -90 Q100 -90 100 -50 L100 0 Q100 40 60 40 L0 40 L-40 80 L-30 40 L-40 40 Q-80 40 -80 0 Z',
				C.paper
			)
		);
		out += at(710, 278, 0.42, 0, heart(C.pink));
		out += petals([
			[300, 260, 1.3, 20],
			[260, 420, 1, -30],
			[360, 200, 0.9, 60],
			[780, 470, 1.1, 10]
		]);
		return out;
	},
	'anime.10': () => {
		let out = glow(512, 460, 320, '#ffc4d8', 0.22);
		out += at(512, 560, 1, 0, pond(330, 150, { ripples: true }));
		out += at(380, 520, 1.5, 10, tadpole({ color: '#7fcf6f' }));
		out += at(600, 600, 1.7, -8, tadpole());
		out += at(640, 470, 1.2, 20, tadpole({ color: '#9ee08a' }));
		out +=
			at(300, 640, 1, 0, blossom(34)) +
			at(720, 540, 1, 30, blossom(28)) +
			at(500, 440, 1, 10, blossom(24));
		out += petals([
			[280, 300, 1.2, 20],
			[420, 250, 1, -40],
			[700, 300, 1.3, 50],
			[760, 380, 0.9, -10]
		]);
		return out;
	},
	'anime.25': () => {
		let out = glow(512, 480, 340, '#ffc4d8', 0.3);
		out += line('M240 200 Q340 240 360 320 M300 220 Q300 280 260 320', '#7a4a3a', 14);
		out +=
			at(330, 270, 1, 0, blossom(40)) +
			at(250, 330, 1, 0, blossom(30)) +
			at(390, 210, 1, 0, blossom(28));
		out += at(
			560,
			520,
			1.1,
			0,
			tv(380, 280, {
				screen: at(0, 10, 1.9, 0, frog({ pose: 'head', eyes: 'starry', mouth: 'w' }))
			})
		);
		out += petals([
			[700, 260, 1.4, 20],
			[760, 400, 1.2, -30],
			[640, 340, 1, 60],
			[300, 520, 1.3, 10],
			[260, 660, 1.1, -40]
		]);
		return out;
	},
	'anime.50': () => {
		let out = glow(512, 420, 340, '#ffc4d8', 0.34);
		out += line(
			'M140 180 Q320 200 420 300 M260 190 Q300 120 380 110 M330 250 Q420 220 500 230',
			'#6b3e2e',
			18
		);
		for (const [x, y, r] of [
			[200, 190, 42],
			[300, 150, 36],
			[380, 110, 32],
			[420, 290, 40],
			[500, 230, 34],
			[340, 240, 30],
			[250, 250, 28],
			[460, 180, 26]
		])
			out += at(x, y, 1, x % 70, blossom(r));
		out += padBase(690, 230, { flower: true });
		out += putFrog(560, 520, 3.3, { eyes: 'happy', mouth: 'smile', look: [-1, -2] });
		out += petals([
			[640, 300, 1.3, 20],
			[740, 380, 1.1, -30],
			[700, 230, 1, 60],
			[300, 440, 1.2, 10],
			[380, 520, 0.9, -40],
			[760, 520, 1, 30]
		]);
		return out;
	},
	'anime.100': () => {
		let out = glow(512, 460, 340, '#ffb14a', 0.36);
		out += at(512, 450, 1, 0, burst(330, '#ffd25e', { rays: 16 }));
		for (let i = 0; i < 14; i++) {
			const a = ((i * 360) / 14 + 7) * (Math.PI / 180);
			out += line(
				`M${f(512 + Math.cos(a) * 250)} ${f(450 + Math.sin(a) * 250)} L${f(512 + Math.cos(a) * 400)} ${f(450 + Math.sin(a) * 400)}`,
				'#ffffff',
				6,
				{ opacity: 0.35 }
			);
		}
		out += shadow(512, 744, 160, 16);
		out += putFrog(512, 520, 3.9, { arms: 'up', wear: ['headband'], eyes: 'wide', mouth: 'grin' });
		out += sparkles([
			[290, 300, 26],
			[740, 290, 22],
			[770, 520, 14]
		]);
		return out;
	},
	'anime.250': () => {
		let out = glow(512, 440, 340, '#b48cff', 0.34);
		out += stars([
			[260, 260, 14],
			[770, 250, 18],
			[700, 170, 6],
			[320, 180, 5],
			[800, 400, 5]
		]);
		out += at(512, 660, 1.5, 0, lotus());
		out += putFrog(512, 470, 3.5, {
			eyes: 'closed',
			mouth: 'smile',
			wear: ['wizard'],
			arms: 'hold'
		});
		out +=
			at(300, 520, 1, 0, blossom(36)) +
			at(724, 520, 1, 20, blossom(36)) +
			at(250, 420, 1, 40, blossom(24)) +
			at(774, 420, 1, 10, blossom(24));
		out += petals([
			[360, 640, 1, 20],
			[680, 650, 1.1, -30]
		]);
		return out;
	},

	// seasons -------------------------------------------------------------------------
	'seasons.10': () => {
		const cx = 512;
		const cy = 440;
		const r = 270;
		const wedge = (a0, a1, color) => {
			const p = (a) =>
				`${f(cx + Math.cos((a * Math.PI) / 180) * r)} ${f(cy + Math.sin((a * Math.PI) / 180) * r)}`;
			return path(`M${cx} ${cy} L${p(a0)} A${r} ${r} 0 0 1 ${p(a1)} Z`, color, { opacity: 0.55 });
		};
		let out =
			wedge(-90, 0, '#ffc4d8') +
			wedge(0, 90, '#f2a65a') +
			wedge(90, 180, '#cfe9ff') +
			wedge(180, 270, '#ffe68a');
		out += circle(cx, cy, r, 'none', { stroke: '#ffffff', sw: 8, opacity: 0.4 });
		out += at(640, 300, 1, 0, blossom(40));
		out += at(
			650,
			590,
			1,
			30,
			ellipse(0, 0, 44, 22, C.orange) + line('M-44 0 L44 0', C.redDeep, 4)
		);
		out +=
			circle(370, 580, 16, '#ffffff') +
			circle(330, 620, 11, '#ffffff') +
			circle(400, 630, 9, '#ffffff');
		out += at(380, 300, 1, 0, sun(48));
		out += putFrog(512, 470, 2.8, { eyes: 'happy', mouth: 'grin' });
		return out;
	},
	'seasons.50': () => {
		let out = glow(512, 420, 340, '#ffe9a8', 0.22);
		out += at(512, 680, 1, 0, pond(380, 90, { ripples: true }));
		out += at(250, 650, 1, 0, lilyPad(110, 32, { color: '#ff9fbf', deep: '#d96a8f' }));
		out += at(512, 690, 1, 0, lilyPad(120, 34));
		out += at(774, 650, 1, 0, lilyPad(110, 32, { color: '#f2a65a', deep: '#c47a2e' }));
		out += line('M250 600 Q380 300 512 560', '#ffffff', 8, { opacity: 0.5, dash: '18 18' });
		out += line('M512 560 Q640 300 740 520', '#ffffff', 8, { opacity: 0.5, dash: '18 18' });
		out += at(250, 606, 1, 0, blossom(26)) + at(780, 600, 1, 20, ellipse(0, 0, 30, 15, C.orange));
		out += putFrog(640, 380, 3.1, { pose: 'leap', eyes: 'happy', mouth: 'open' }, 20);
		return out;
	},
	'seasons.100': () => {
		let out = glow(512, 450, 330, '#ffe9a8', 0.26);
		out += at(512, 470, 1, 0, laurel(-1, 300) + laurel(1, 300));
		out += shadow(512, 730, 150, 14);
		out += putFrog(512, 500, 3.6, { wear: ['sprout'], eyes: 'happy', mouth: 'grin', arms: 'up' });
		out +=
			at(300, 300, 1, 0, blossom(34)) +
			at(724, 300, 1, 30, blossom(34)) +
			at(270, 560, 1, 10, blossom(26)) +
			at(754, 560, 1, 50, blossom(26));
		out += petals([
			[400, 220, 1, 20],
			[640, 210, 1.1, -30]
		]);
		return out;
	},
	'seasons.250': () => {
		let out = sky('#16304a', '#2a5a6a');
		out += stars([
			[240, 260, 5],
			[300, 190, 3],
			[800, 300, 4],
			[720, 200, 12]
		]);
		out += inDisc(ellipse(512, 760, 460, 120, '#e8f3ff'));
		const tree = (x, y, s) =>
			at(
				x,
				y,
				s,
				0,
				rect(-16, 120, 32, 70, 6, '#6b3e2e') +
					poly(
						[
							[0, -190],
							[110, -20],
							[-110, -20]
						],
						'#2f7d46'
					) +
					poly(
						[
							[0, -120],
							[140, 60],
							[-140, 60]
						],
						'#3a9055'
					) +
					poly(
						[
							[0, -50],
							[170, 140],
							[-170, 140]
						],
						'#2f7d46'
					) +
					poly(
						[
							[0, -190],
							[40, -130],
							[-40, -130]
						],
						'#ffffff',
						{ opacity: 0.85 }
					)
			);
		out += tree(640, 440, 1.25);
		out += tree(820, 520, 0.7);
		out += putFrog(380, 560, 3.1, {
			wear: ['scarf'],
			eyes: 'happy',
			mouth: 'smile',
			look: [2, -1]
		});
		out +=
			circle(260, 380, 7, '#ffffff') +
			circle(470, 300, 6, '#ffffff') +
			circle(560, 220, 8, '#ffffff') +
			circle(330, 450, 5, '#ffffff');
		return out;
	},
	'seasons.500': () => {
		let out = sky('#1a2c4a', '#24506a');
		out += at(700, 260, 1, -20, moon(70));
		out += stars([
			[300, 220, 5],
			[380, 160, 3],
			[820, 400, 4],
			[250, 340, 12]
		]);
		out += inDisc(
			rect(
				60,
				560,
				904,
				404,
				0,
				linear('oldpond', [
					[0, '#2c8a83'],
					[1, '#154a47']
				])
			)
		);
		for (const [rx, o] of [
			[120, 0.7],
			[220, 0.45],
			[330, 0.25]
		])
			out += ellipse(512, 620, rx, rx * 0.22, 'none', { stroke: '#cfe9ff', sw: 7, opacity: o });
		out +=
			circle(470, 560, 12, '#cfe9ff') +
			circle(560, 540, 9, '#cfe9ff') +
			circle(520, 520, 14, '#cfe9ff') +
			circle(600, 570, 7, '#cfe9ff');
		out += putFrog(470, 400, 3, { pose: 'leap', eyes: 'closed', mouth: 'smile' }, 35);
		out +=
			at(260, 680, 1, 0, lilyPad(90, 24)) + at(790, 700, 1, 0, lilyPad(80, 22, { flower: true }));
		return out;
	},

	// days ----------------------------------------------------------------------------
	'days.30': () => {
		let out = glow(512, 450, 300, '#ffe68a', 0.24);
		out += shadow(512, 744, 170, 14);
		let grid = '';
		for (let r = 0; r < 3; r++)
			for (let c = 0; c < 5; c++)
				grid += at(
					-104 + c * 52,
					-50 + r * 50,
					0.2,
					0,
					check(r === 2 && c > 2 ? '#d8d2c0' : C.pad, 44)
				);
		out += at(512, 480, 1, 0, calendar(340, 340, { inside: grid }));
		out += at(680, 316, 1.8, 18, peek({ eyes: 'happy' }));
		return out;
	},
	'days.100': () => {
		const chair = armchair({ color: '#3f7fe0', deep: '#2d5fb0', back: '#3466c0', seat: '#5a92f0' });
		let out = glow(512, 380, 300, '#ffe9a8', 0.24);
		out += at(512, 470, 0.9, 0, chair.back);
		out += putFrog(512, 440, 3.1, { arms: 'hold', eyes: 'happy', mouth: 'smile' });
		out += at(512, 470, 0.9, 0, chair.front ?? '');
		out += at(512, 560, 0.42, 0, mug(C.red));
		out += line('M490 470 Q480 440 496 420 M530 470 Q520 440 536 420', '#ffffff', 6, {
			opacity: 0.5
		});
		return out;
	},
	'days.365': () => {
		let out = glow(512, 450, 330, '#ffe68a', 0.26);
		for (let i = 0; i < 12; i++) {
			const a = ((i * 30 - 90) * Math.PI) / 180;
			const colors = [
				'#cfe9ff',
				'#cfe9ff',
				'#ffc4d8',
				'#ffc4d8',
				'#8af0b4',
				'#ffd25e',
				'#ffd25e',
				'#ffd25e',
				'#f2a65a',
				'#f2a65a',
				'#f27a33',
				'#cfe9ff'
			];
			out +=
				circle(512 + Math.cos(a) * 280, 450 + Math.sin(a) * 260, 30, colors[i]) +
				circle(512 + Math.cos(a) * 280, 450 + Math.sin(a) * 260, 30, 'none', {
					stroke: '#ffffff',
					sw: 5,
					opacity: 0.5
				});
		}
		out += at(430, 330, 1, 0, sun(40)) + at(600, 330, 1, -20, moon(36));
		out += putFrog(512, 500, 3.2, { arms: 'up', eyes: 'starry', mouth: 'grin', wear: ['party'] });
		return out;
	},
	'days.1000': () => {
		let out = sky('#101c36', '#1c3450');
		const pts = [];
		for (let i = 0; i < 26; i++)
			pts.push([180 + ((i * 137) % 660), 170 + ((i * 89) % 330), i % 7 === 0 ? 12 : 3 + (i % 3)]);
		out += stars(pts);
		out += at(740, 260, 1, -20, moon(52));
		out += padBase(690, 230);
		out += at(330, 520, 0.5, 0, tv(300, 220, { antenna: false }));
		out += inDisc(
			poly(
				[
					[400, 470],
					[640, 380],
					[640, 700],
					[400, 580]
				],
				radial(
					'tvglow1k',
					[
						[0, '#5fd3ff', 0.4],
						[1, '#5fd3ff', 0]
					],
					{ cx: 0, cy: 0.5, r: 1 }
				)
			)
		);
		out += putFrog(580, 520, 3.2, { arms: 'hold', eyes: 'happy', mouth: 'smile', look: [-2, 0] });
		return out;
	},

	// notes ---------------------------------------------------------------------------
	'notes.10': () => {
		let out = glow(512, 450, 300, '#fff3c4', 0.22);
		out += shadow(512, 744, 180, 14);
		out += at(
			560,
			520,
			1,
			6,
			rect(-150, -190, 300, 380, 18, C.paper) +
				rect(-150, -190, 34, 380, 12, C.pad) +
				line('M-90 -110 L110 -110 M-90 -50 L110 -50 M-90 10 L90 10 M-90 70 L60 70', '#b9b2a0', 8)
		);
		out += putFrog(360, 560, 2.8, { arms: 'right', eyes: 'happy', mouth: 'smirk' });
		out += at(470, 470, 0.8, -40, pencil());
		return out;
	},
	'notes.50': () => {
		let out = glow(512, 440, 320, '#fff3c4', 0.24);
		out += shadow(512, 744, 200, 16);
		out += at(512, 560, 1.4, 0, openBook({ cover: C.padDeep }));
		out += at(512, 480, 2.3, 0, peek({ eyes: 'happy' }));
		out += at(600, 440, 1, 0, line('M0 0 L0 170', C.red, 18));
		out += sparkles(
			[
				[320, 330, 24],
				[710, 320, 20],
				[760, 470, 12]
			],
			'#fff3c4'
		);
		return out;
	},
	'notes.250': () => {
		let out = glow(512, 430, 330, '#fff3c4', 0.26);
		out +=
			rect(300, 250, 424, 420, 0, C.paper) +
			rect(280, 220, 464, 50, 25, '#d8c7a0') +
			rect(280, 650, 464, 50, 25, '#d8c7a0');
		out += line(
			'M340 320 L680 320 M340 370 L660 370 M340 420 L690 420 M340 470 L620 470 M340 520 L670 520 M340 570 L600 570',
			'#b9b2a0',
			8
		);
		out += putFrog(660, 560, 2.6, {
			wear: ['monocle'],
			arms: 'left',
			eyes: 'happy',
			mouth: 'smile'
		});
		out += at(
			590,
			470,
			1,
			-35,
			path('M0 -130 Q70 -60 12 60 L-8 60 Q-40 -40 0 -130 Z', C.teal) +
				line('M0 -110 L2 56', '#1f7f8e', 5) +
				line('M2 60 L-4 116', '#23342b', 8)
		);
		return out;
	},

	// more anime and moments -------------------------------------------------------
	simulcast: () => {
		let out = inDisc(
			rect(
				60,
				60,
				904,
				904,
				0,
				linear('simdawn', [
					[0, '#2b3f6a'],
					[0.6, '#ff9a8a'],
					[1, '#ffd2b0']
				])
			)
		);
		out += circle(512, 380, 220, '#e4402e', { opacity: 0.9 });
		out += petals([
			[250, 300, 1.4, 20],
			[780, 300, 1.2, -30],
			[280, 560, 1.1, 50],
			[760, 600, 1.3, 10]
		]);
		out += at(
			512,
			580,
			1.1,
			0,
			tv(400, 290, {
				screen: at(0, 10, 2, 0, frog({ pose: 'head', eyes: 'starry', mouth: 'open' }))
			})
		);
		out += at(740, 420, 1, 0, clock(70, { face: '#fff6dc' }));
		return out;
	},
	'cour-keeper': () => {
		let out = glow(512, 460, 330, '#b48cff', 0.26);
		for (let r = 0; r < 3; r++)
			for (let c = 0; c < 4; c++) {
				const x = 320 + c * 128;
				const y = 290 + r * 110;
				out +=
					rect(x - 52, y - 40, 104, 80, 14, '#23342b') +
					rect(x - 44, y - 32, 88, 64, 10, '#5a9ad8') +
					at(x, y, 0.22, 0, check('#ffffff', 40));
			}
		out += at(260, 260, 1, 0, blossom(30)) + at(770, 640, 1, 30, blossom(26));
		out += at(512, 690, 3, 0, peek({ eyes: 'starry' }));
		out += rect(300, 690, 424, 24, 12, C.padDeep);
		return out;
	},
	nocturnal: () => {
		let out = sky('#0d1630', '#1a2a4a');
		const ring = [];
		for (let i = 0; i < 10; i++) {
			const a = ((200 + i * 15.5) * Math.PI) / 180;
			ring.push([512 + Math.cos(a) * 330, 520 + Math.sin(a) * 330, 12]);
		}
		out += stars(ring);
		out += at(512, 330, 1, 0, moon(90));
		out += padBase(700, 220);
		out += putFrog(512, 540, 3.6, {
			eyes: 'wide',
			mouth: 'small',
			wear: ['nightcap'],
			arms: 'hold',
			colors: { skin: '#5fbf9a', skinDeep: '#3f9a7a', belly: '#8ad8c0' }
		});
		out += zed(690, 470, 34) + zed(740, 420, 24);
		return out;
	},
	'tadpole-to-frog': () => {
		let out = glow(512, 460, 340, '#8ff0ff', 0.2);
		out += at(512, 700, 1, 0, pond(400, 90, { ripples: true }));
		out += at(220, 600, 1.2, 0, tadpole());
		out += at(
			400,
			560,
			1.45,
			0,
			tadpole({ color: '#7fcf6f' }) +
				ellipse(-20, 40, 10, 5, C.skin) +
				ellipse(20, 40, 10, 5, C.skin)
		);
		out += at(690, 690, 1, 0, lilyPad(140, 38, { flower: true }));
		out += putFrog(690, 560, 3, { arms: 'wave', eyes: 'starry', mouth: 'grin', wear: ['crown'] });
		out += line('M250 520 Q380 360 520 420', '#ffffff', 8, { opacity: 0.45, dash: '16 16' });
		out += at(
			380,
			300,
			0.5,
			0,
			tv(260, 190, { antenna: false, screen: at(0, 6, 1, 0, star(30, C.yellow)) })
		);
		out += at(
			640,
			270,
			0.5,
			0,
			tv(260, 190, {
				antenna: false,
				screen: at(0, 6, 1, 0, flag(60, { checkered: true, id: 'ttf' }))
			})
		);
		return out;
	},
	'anime-film-club': () => {
		let out = glow(512, 450, 330, '#ffc4d8', 0.28);
		out += shadow(512, 744, 180, 14);
		out += at(512, 450, 1, 0, reel(230));
		out += at(512, 450, 2.6, 0, frog({ pose: 'head', eyes: 'starry', mouth: 'w' }));
		out += at(300, 640, 1, -10, clapper());
		out +=
			at(260, 270, 1, 0, blossom(40)) +
			at(760, 280, 1, 30, blossom(34)) +
			at(770, 620, 1, 10, blossom(30));
		out += petals([
			[680, 200, 1.2, 20],
			[340, 180, 1, -30],
			[820, 450, 1, 40]
		]);
		return out;
	},

	// specials ------------------------------------------------------------------------
	'summit-frog': () => {
		let out = inDisc(
			rect(
				60,
				60,
				904,
				904,
				0,
				linear('summitsky', [
					[0, '#5aa8e6'],
					[1, '#cfe9ff']
				])
			)
		);
		out += inDisc(
			poly(
				[
					[60, 900],
					[512, 300],
					[964, 900]
				],
				'#6f8f80'
			) +
				poly(
					[
						[60, 900],
						[300, 600],
						[520, 900]
					],
					'#587a6a'
				) +
				poly(
					[
						[512, 300],
						[600, 420],
						[560, 400],
						[520, 440],
						[470, 400],
						[430, 420]
					],
					'#ffffff'
				)
		);
		out +=
			line('M600 330 L600 120', '#6b3e2e', 12) +
			at(600, 120, 1, 0, path('M0 0 L120 34 L0 76 Z', C.red));
		out += putFrog(490, 250, 2.6, { arms: 'right', eyes: 'starry', mouth: 'grin' });
		out += sparkles([
			[700, 200, 22],
			[300, 260, 18]
		]);
		return out;
	},
	'frog-prince': () => {
		let out = glow(512, 450, 340, '#ffe68a', 0.32);
		for (let i = 0; i < 5; i++) {
			const a = ((200 + i * 35) * Math.PI) / 180;
			const x = 512 + Math.cos(a) * 320;
			const y = 470 + Math.sin(a) * 300;
			out +=
				circle(x, y, 40, C.gold, { stroke: C.goldDeep, sw: 6 }) +
				at(x, y, 1, 0, star(18, '#fff3c4'));
		}
		out += at(512, 700, 1.2, 0, cushion('#8f2a5a'));
		out += line('M380 790 L360 830 M644 790 L664 830', C.gold, 10);
		out += putFrog(512, 540, 3.6, { wear: ['crown', 'scarf'], eyes: 'happy', mouth: 'smile' });
		out += at(680, 440, 0.4, 0, heart(C.red));
		return out;
	},
	'pond-legend': () => {
		let out = glow(512, 440, 360, '#ffe68a', 0.34);
		const rings = [
			'#d08c4e',
			'#c3cad2',
			'#f2c444',
			'#3fbf86',
			'#e2405a',
			'#4b86e6',
			'#a26de6',
			'#8ff0ff',
			'#ffc4d8',
			'#ffffff'
		];
		for (let i = 0; i < 10; i++) {
			const a = ((i * 36 - 90) * Math.PI) / 180;
			out +=
				circle(512 + Math.cos(a) * 330, 460 + Math.sin(a) * 310, 26, rings[i]) +
				circle(512 + Math.cos(a) * 330, 460 + Math.sin(a) * 310, 17, '#18392a');
		}
		out += at(
			512,
			500,
			1,
			0,
			laurel(-1, 260, { color: '#f2c444', dark: '#d9a520', stem: '#b07d12' }) +
				laurel(1, 260, { color: '#f2c444', dark: '#d9a520', stem: '#b07d12' })
		);
		out += rect(392, 640, 240, 70, 14, '#b07d12') + rect(412, 620, 200, 40, 12, '#d9a520');
		out += putFrog(512, 470, 3.4, { arms: 'up', eyes: 'happy', mouth: 'grin', colors: GOLD_FROG });
		out += sparkles([
			[330, 300, 24],
			[700, 290, 28],
			[760, 560, 16]
		]);
		return out;
	}
};

export { f };
