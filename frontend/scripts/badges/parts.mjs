/**
 * The pieces every badge scene is built from. Each is drawn around its own
 * origin at a fixed size and placed with `at(x, y, scale, turn, …)`, so one
 * popcorn bucket or one frog looks the same on every badge it appears on.
 *
 * The frog is the mascot's own geometry (`mascots.mjs`, a 64 grid) with its
 * head's centre as the origin, given a body, arms and things to wear.
 */

import {
	C,
	at,
	circle,
	ellipse,
	f,
	glint,
	group,
	line,
	linear,
	path,
	poly,
	radial,
	rect,
	starPoints
} from './draw.mjs';

// --- the frog -------------------------------------------------------------

function eyes(kind, c, look = [0, 0]) {
	const [dx, dy] = look;
	const whites = circle(-13, -18, 8, c.eyeWhite) + circle(13, -18, 8, c.eyeWhite);
	const pupils = (r = 4.2, ox = 1.2, oy = 1) =>
		circle(-13 + ox + dx, -18 + oy + dy, r, c.pupil) +
		circle(13 - ox + dx, -18 + oy + dy, r, c.pupil) +
		circle(-14.6 + dx, -19.8 + dy, r * 0.4, c.shine) +
		circle(11.4 + dx, -19.8 + dy, r * 0.4, c.shine);
	switch (kind) {
		case 'closed':
			return (
				line('M-18.5 -18.5 Q-13 -12.5 -7.5 -18.5', c.pupil, 2.8) +
				line('M7.5 -18.5 Q13 -12.5 18.5 -18.5', c.pupil, 2.8)
			);
		case 'happy':
			return (
				line('M-19 -15.5 Q-13 -23.5 -7 -15.5', c.pupil, 3.2) +
				line('M7 -15.5 Q13 -23.5 19 -15.5', c.pupil, 3.2)
			);
		case 'sleepy':
			return (
				whites +
				pupils(4, 1, 2.6) +
				path('M-21 -18 A8 8 0 0 1 -5 -18 Z', c.skinDeep) +
				path('M5 -18 A8 8 0 0 1 21 -18 Z', c.skinDeep) +
				line('M-21 -18 L-5 -18 M5 -18 L21 -18', c.pupil, 1.6)
			);
		case 'starry':
			return (
				circle(-13, -18, 8.6, c.eyeWhite) +
				circle(13, -18, 8.6, c.eyeWhite) +
				circle(-12.4 + dx, -17 + dy, 6.2, c.pupil) +
				circle(12.4 + dx, -17 + dy, 6.2, c.pupil) +
				circle(-14.8 + dx, -19.6 + dy, 2.4, c.shine) +
				circle(10 + dx, -19.6 + dy, 2.4, c.shine) +
				circle(-10.2 + dx, -14.4 + dy, 1.2, c.shine) +
				circle(14.6 + dx, -14.4 + dy, 1.2, c.shine)
			);
		case 'wide':
			return (
				circle(-13, -18, 9, c.eyeWhite) +
				circle(13, -18, 9, c.eyeWhite) +
				circle(-13 + dx, -18 + dy, 3, c.pupil) +
				circle(13 + dx, -18 + dy, 3, c.pupil)
			);
		case 'stars': {
			const s = (x) => at(x + dx, -18 + dy, 1, 0, poly(starPoints(5, 6.5, 0.45), C.yellow));
			return whites + s(-13) + s(13);
		}
		case 'hearts':
			return (
				whites +
				at(-13 + dx, -17.5 + dy, 0.13, 0, heart(C.red)) +
				at(13 + dx, -17.5 + dy, 0.13, 0, heart(C.red))
			);
		case 'focus':
			return (
				whites +
				pupils(4.2, 1.2, 1.5) +
				path('M-21.5 -19 L-4.5 -23 L-4.5 -27 L-21.5 -27 Z', c.skin) +
				path('M4.5 -23 L21.5 -19 L21.5 -27 L4.5 -27 Z', c.skin) +
				line('M-21 -19.5 L-5 -23.2 M5 -23.2 L21 -19.5', c.pupil, 1.8)
			);
		case 'wink':
			return (
				circle(-13, -18, 8, c.eyeWhite) +
				circle(-11.8 + dx, -17 + dy, 4.2, c.pupil) +
				circle(-13.8 + dx, -19.2 + dy, 1.7, c.shine) +
				line('M7 -15.5 Q13 -23.5 19 -15.5', c.pupil, 3.2)
			);
		default:
			return whites + pupils();
	}
}

function mouth(kind, c) {
	switch (kind) {
		case 'small':
			return line('M-6 5 Q0 9.5 6 5', c.mouth, 2.8);
		case 'grin':
			return (
				path('M-8 2.5 Q0 2.5 8 2.5 Q8 12 0 12 Q-8 12 -8 2.5 Z', c.mouth) +
				ellipse(0, 9.6, 3.6, 2, c.blush)
			);
		case 'open':
			return ellipse(0, 6, 5, 4.6, c.mouth) + ellipse(0, 8.2, 3, 1.8, c.blush);
		case 'o':
			return ellipse(0, 6, 2.8, 3.4, c.mouth);
		case 'flat':
			return line('M-6 6 L6 6', c.mouth, 2.8);
		case 'smirk':
			return line('M-7 6.5 Q1 9.5 8 3', c.mouth, 2.8);
		case 'w':
			return line('M-6 5 Q-3 9.5 0 5.5 Q3 9.5 6 5', c.mouth, 2.6);
		case 'wobble':
			return line('M-7 7 Q-3.5 4 0 7 Q3.5 10 7 7', c.mouth, 2.6);
		case 'none':
			return '';
		default:
			return line('M-9 3.5 Q0 12.5 9 3.5', c.mouth, 3);
	}
}

function wear(item, c) {
	switch (item) {
		case 'glasses3d':
			return (
				rect(-23, -24.5, 20, 13, 4, c.lensRed, { opacity: 0.8 }) +
				rect(3, -24.5, 20, 13, 4, c.lensCyan, { opacity: 0.8 }) +
				rect(-4, -19.5, 8, 2.6, 1.3, c.frame) +
				line('M-23 -18 L-28 -16 M23 -18 L28 -16', c.frame, 2.4)
			);
		case 'sunglasses':
			return (
				path('M-24 -23 L-3 -23 L-4 -16 Q-6 -10 -13 -10 Q-21 -10 -23 -16 Z', c.ink) +
				path('M24 -23 L3 -23 L4 -16 Q6 -10 13 -10 Q21 -10 23 -16 Z', c.ink) +
				line('M-3 -21 L3 -21', c.ink, 2.4) +
				line('M-19 -20 L-14 -20 M7 -20 L12 -20', '#ffffff', 1.8, { opacity: 0.6 })
			);
		case 'crown':
			return (
				rect(-6, -27, 12, 9, 2, c.skin) +
				path('M-12 -24 L-15 -41 L-6.5 -33 L0 -45 L6.5 -33 L15 -41 L12 -24 Z', C.gold, {
					stroke: C.goldDeep,
					sw: 1.2
				}) +
				circle(0, -30, 2.4, C.red) +
				circle(-8.5, -28.5, 1.6, C.teal) +
				circle(8.5, -28.5, 1.6, C.teal) +
				circle(0, -45, 1.8, '#fff3b8') +
				circle(-15, -41, 1.5, '#fff3b8') +
				circle(15, -41, 1.5, '#fff3b8')
			);
		case 'wizard':
			return (
				path('M-21 -27 L6 -78 L19 -27 Z', '#5b3fa6') +
				path('M6 -78 L19 -27 L8 -27 Z', '#47318a') +
				at(-2, -50, 1, 0, poly(starPoints(5, 4.5), C.yellow)) +
				at(7, -38, 1, 0, poly(starPoints(5, 3), C.yellow)) +
				circle(-6, -62, 1.6, C.yellow) +
				ellipse(-1, -27, 27, 5.5, '#47318a')
			);
		case 'party':
			return (
				rect(-6, -27, 12, 9, 2, c.skin) +
				path('M-10 -25 L1 -56 L11 -25 Z', C.pink) +
				line('M-6 -35 L8 -37 M-3 -45 L6 -46', '#ffffff', 2.4, { opacity: 0.85 }) +
				circle(1, -57, 4, C.yellow)
			);
		case 'nightcap':
			return (
				path('M-24 -24 Q-14 -54 14 -50 Q34 -46 40 -18 Q30 -30 20 -32 Q8 -26 -24 -24 Z', '#3f7fe0') +
				circle(40, -16, 5, '#ffffff') +
				path('M-26 -22 Q0 -32 24 -28 L24 -22 Q0 -26 -26 -16 Z', '#ffffff')
			);
		case 'captain':
			return (
				path('M-22 -26 Q0 -48 22 -26 Z', '#ffffff') +
				rect(-23, -28, 46, 6, 3, C.ink) +
				path('M-24 -24 Q0 -16 26 -24 L28 -20 Q0 -10 -26 -20 Z', C.ink) +
				circle(0, -34, 3.4, C.gold)
			);
		case 'headband':
			return (
				line('M-25 -5 Q0 -12 25 -5', C.red, 6.5, { cap: 'butt' }) +
				path('M23 -7 L36 -14 L33 -4 Z', C.red) +
				path('M23 -5 L35 2 L28 -1 Z', C.redDeep) +
				circle(0, -9, 2.6, '#ffffff')
			);
		case 'beret':
			return (
				rect(-6, -27, 12, 8, 2, c.skin) +
				at(3, -30, 1, -12, ellipse(0, 0, 19, 7, C.red)) +
				line('M3 -36 L4 -40', C.redDeep, 2.4)
			);
		case 'bow':
			return (
				rect(-5, -26, 10, 9, 2, c.skin) +
				ellipse(-6.5, -26.5, 6.5, 4.8, c.bow) +
				ellipse(6.5, -26.5, 6.5, 4.8, c.bow) +
				circle(0, -25, 4.2, c.bowKnot)
			);
		case 'sprout':
			return (
				line('M0 -19 Q-1 -25 0 -31', c.stem, 2.4) +
				ellipse(-5.5, -31.5, 5.5, 3.4, c.leaf) +
				ellipse(5.5, -31.5, 5.5, 3.4, c.leaf)
			);
		case 'headset':
			return (
				line('M-25 -8 Q-27 -38 0 -40 Q27 -38 25 -8', C.ink, 3.6) +
				rect(-30, -14, 9, 15, 4, C.ink) +
				rect(21, -14, 9, 15, 4, C.ink) +
				line('M-26 0 Q-24 12 -10 12', C.ink, 2.4) +
				circle(-9, 12, 2.6, C.red)
			);
		case 'wig':
			return group(
				[
					[-26, -10],
					[-28, 0],
					[-27, 10],
					[26, -10],
					[28, 0],
					[27, 10],
					[-20, -27],
					[20, -27],
					[-8, -30],
					[8, -30]
				]
					.map(([x, y]) => circle(x, y, 7, '#f4f1ea', { stroke: '#cfc8b6', sw: 1.2 }))
					.join('')
			);
		case 'grad':
			return (
				rect(-6, -27, 12, 8, 2, c.skin) +
				poly(
					[
						[0, -42],
						[24, -34],
						[0, -26],
						[-24, -34]
					],
					C.ink
				) +
				rect(-12, -32, 24, 7, 2, C.ink) +
				line('M18 -34 L22 -22', C.yellow, 1.8) +
				circle(22, -21, 2, C.yellow)
			);
		case 'halo':
			return ellipse(0, -40, 18, 4.5, 'none', { stroke: C.yellow, sw: 3 });
		case 'scarf':
			return (
				path('M-16 17 Q0 25 16 17 L17 22 Q0 31 -17 22 Z', C.red) +
				path('M8 23 L14 38 L6 37 L3 24 Z', C.redDeep)
			);
		case 'medal':
			return (
				line('M-6 16 L0 28 L6 16', C.blue, 3) +
				circle(0, 31, 5, C.gold, { stroke: C.goldDeep, sw: 1 })
			);
		case 'antenna':
			return line('M0 -20 L0 -40', '#9aa6a0', 2.2) + circle(0, -42, 3.4, C.red);
		case 'monocle':
			return (
				circle(13, -18, 9.5, 'none', { stroke: C.gold, sw: 2 }) +
				line('M22 -15 Q26 0 22 14', C.gold, 1.2)
			);
		case 'blush':
			return (
				ellipse(-19.5, 2, 6, 3.8, c.blush, { opacity: 0.95 }) +
				ellipse(19.5, 2, 6, 3.8, c.blush, { opacity: 0.95 })
			);
		case 'snowcap':
			return path(
				'M-24 -24 Q-14 -34 -2 -28 Q8 -36 18 -28 Q26 -32 28 -22 Q0 -18 -24 -24 Z',
				'#ffffff'
			);
		default:
			return '';
	}
}

/**
 * A frog. `pose`: sit (whole body), head, leap, swim. `arms`: down, up,
 * right, left, hold (both hands in front of the chest, at y 38), wave,
 * none. `eyes`, `mouth`, `wear` (a list) and `look` ([dx, dy] for the pupils).
 */
export function frog(o = {}) {
	const c = { ...C, ...(o.colors ?? {}) };
	const pose = o.pose ?? 'sit';
	const arms = o.arms ?? 'down';
	let out = '';

	if (pose === 'sit') {
		out += ellipse(-20, 41, 11, 8.5, c.skinDeep) + ellipse(20, 41, 11, 8.5, c.skinDeep);
		out += ellipse(-24, 49, 10, 4.2, c.skinDeep) + ellipse(24, 49, 10, 4.2, c.skinDeep);
		out += ellipse(0, 31, 20, 17.5, c.skin);
		out += ellipse(0, 35, 12.5, 12, c.belly, { opacity: 0.9 });
	} else if (pose === 'leap') {
		out += at(-30, 44, 1, 32, ellipse(0, 0, 22, 6.5, c.skinDeep));
		out += at(-48, 58, 1, 12, ellipse(0, 0, 10, 4, c.skinDeep));
		out += at(-16, 48, 1, 52, ellipse(0, 0, 20, 6, c.skinDeep));
		out += at(-24, 64, 1, 30, ellipse(0, 0, 9, 3.6, c.skinDeep));
		out += at(-6, 26, 1, 30, ellipse(0, 0, 24, 15, c.skin));
		out += at(-3, 30, 1, 30, ellipse(0, 0, 15, 9, c.belly, { opacity: 0.9 }));
	} else if (pose === 'swim') {
		out += at(-44, 10, 1, -18, ellipse(0, 0, 20, 5.5, c.skinDeep));
		out += at(-44, 26, 1, 18, ellipse(0, 0, 20, 5.5, c.skinDeep));
		out += ellipse(-22, 18, 23, 13, c.skin);
	}

	const armDown = (s) =>
		ellipse(s * 11.5, 39, 4.6, 8.5, c.skin) + ellipse(s * 12, 46.5, 5.2, 3, c.skinDeep);
	const armUp = (s) =>
		line(`M${s * 14} 26 Q${s * 25} 18 ${s * 29} 4`, c.skin, 9) + circle(s * 29, 3, 5.8, c.skin);
	const armHold = (s) =>
		line(`M${s * 15} 27 Q${s * 17} 38 ${s * 7} 38`, c.skin, 9) + circle(s * 6, 37.5, 5.6, c.skin);
	const armWave = (s) =>
		line(`M${s * 15} 27 Q${s * 30} 26 ${s * 33} 12`, c.skin, 9) + circle(s * 33, 11, 5.8, c.skin);
	if (pose === 'sit' || pose === 'leap') {
		const pick = { down: armDown, up: armUp, hold: armHold, wave: armWave };
		const L = arms === 'right' || arms === 'wave' ? 'down' : arms === 'left' ? 'up' : arms;
		const R = arms === 'right' ? 'up' : arms === 'wave' ? 'wave' : arms === 'left' ? 'down' : arms;
		if (arms !== 'none' && pose === 'sit')
			out += (pick[L] ?? armDown)(-1) + (pick[R] ?? armDown)(1);
		if (pose === 'leap')
			out +=
				at(16, 38, 1, -30, ellipse(0, 0, 11, 4.5, c.skin)) +
				at(4, 44, 1, -20, ellipse(0, 0, 10, 4.2, c.skin));
	}

	// The head, as the mascot draws it.
	out += ellipse(0, 0, 25, 20, c.skin);
	out += ellipse(0, 7, 15.5, 11, c.belly, { opacity: 0.45 });
	out += circle(-13, -18, 12, c.skin) + circle(13, -18, 12, c.skin);
	out += ellipse(-9, -6, 9, 4, '#ffffff', { opacity: 0.12 });
	out +=
		ellipse(-19.5, 2, 5, 3.3, c.blush, { opacity: 0.82 }) +
		ellipse(19.5, 2, 5, 3.3, c.blush, { opacity: 0.82 });
	const worn = o.wear ?? [];
	// Things that sit under the eyes go first.
	out += worn
		.filter((w) => ['wig', 'scarf', 'medal'].includes(w))
		.map((w) => wear(w, c))
		.join('');
	out += eyes(o.eyes ?? 'open', c, o.look);
	out += mouth(o.mouth ?? 'smile', c);
	out += worn
		.filter((w) => !['wig', 'scarf', 'medal'].includes(w))
		.map((w) => wear(w, c))
		.join('');
	return out;
}

/** A frog placed with its head's centre at (x, y), `s` pixels to a unit. */
export const putFrog = (x, y, s, o = {}, turn = 0) => at(x, y, s, turn, frog(o));

/** Just the top of a frog's head, peeking over an edge at y = 0. */
export function peek(o = {}) {
	const c = { ...C, ...(o.colors ?? {}) };
	return (
		ellipse(0, 10, 25, 20, c.skin) +
		circle(-13, -8, 12, c.skin) +
		circle(13, -8, 12, c.skin) +
		at(0, 10, 1, 0, eyes(o.eyes ?? 'open', c, o.look))
	);
}

// --- pond things ------------------------------------------------------------

/** A lily pad, `rx` by `ry`, its notch facing `notch` degrees. */
export function lilyPad(rx, ry, o = {}) {
	const color = o.color ?? C.pad;
	const deep = o.deep ?? C.padDeep;
	const a = ((o.notch ?? -60) * Math.PI) / 180;
	const w = 0.2;
	const p1 = [Math.cos(a - w) * rx, Math.sin(a - w) * ry];
	const p2 = [Math.cos(a + w) * rx, Math.sin(a + w) * ry];
	const shape = (dy) =>
		`M0 ${dy} L${f(p2[0])} ${f(p2[1] + dy)} A${rx} ${ry} 0 1 1 ${f(p1[0])} ${f(p1[1] + dy)} Z`;
	let out = path(shape(ry * 0.14), deep);
	out += path(shape(0), color);
	for (let i = 1; i < 7; i++) {
		const b = a + (i * Math.PI * 2) / 7;
		out += line(
			`M0 0 L${f(Math.cos(b) * rx * 0.8)} ${f(Math.sin(b) * ry * 0.8)}`,
			'#ffffff',
			Math.max(2, rx * 0.025),
			{
				opacity: 0.18
			}
		);
	}
	if (o.flower) {
		out += at(rx * 0.45, -ry * 0.35, rx / 160, 0, lotus());
	}
	return out;
}

export function lotus() {
	let out = '';
	for (const [r, x] of [
		[-50, -14],
		[50, 14],
		[-20, -6],
		[20, 6],
		[0, 0]
	]) {
		out += at(x, 0, 1, r, ellipse(0, -18, 10, 22, r === 0 ? '#ffc4d8' : C.pink));
	}
	return out + circle(0, -2, 6, C.yellow);
}

/** Water, an ellipse with ripples, centred on its origin. */
export function pond(rx, ry, o = {}) {
	const fill = radial('pond', [
		[0, '#2c8a83'],
		[0.7, C.water],
		[1, C.waterDeep]
	]);
	let out = ellipse(0, 0, rx, ry, fill);
	if (o.ripples !== false) {
		out += ellipse(-rx * 0.35, -ry * 0.1, rx * 0.22, ry * 0.18, 'none', {
			stroke: '#ffffff',
			sw: 3,
			opacity: 0.22
		});
		out += ellipse(rx * 0.4, ry * 0.25, rx * 0.16, ry * 0.13, 'none', {
			stroke: '#ffffff',
			sw: 3,
			opacity: 0.18
		});
	}
	return out;
}

export function tadpole(o = {}) {
	const body = o.color ?? C.stem;
	let out = path('M-26 -12 C-70 -42 -96 26 -150 -8 C-112 36 -72 0 -26 14 Z', o.tail ?? C.skinDeep);
	out += ellipse(0, 0, 40, 33, body);
	out += ellipse(4, 10, 24, 16, C.belly, { opacity: 0.35 });
	out += circle(14, -12, 11, C.eyeWhite) + circle(32, -6, 10, C.eyeWhite);
	out += circle(17, -10, 5.5, C.pupil) + circle(34, -4, 5, C.pupil);
	out += circle(15, -13, 2, '#ffffff') + circle(32, -7, 1.8, '#ffffff');
	out += line('M14 12 Q24 18 33 11', C.mouth, 3.2);
	return out;
}

/** A cartoon fly, facing right. */
export function fly(o = {}) {
	return (
		at(-6, -14, 1, -25, ellipse(0, 0, 14, 8, '#ffffff', { opacity: 0.75 })) +
		at(6, -14, 1, 25, ellipse(0, 0, 14, 8, '#ffffff', { opacity: 0.75 })) +
		ellipse(0, 0, 14, 10, o.color ?? '#23342b') +
		circle(11, -3, 6, o.color ?? '#23342b') +
		circle(13, -5, 2.4, '#ff6b6b')
	);
}

// --- cinema ---------------------------------------------------------------

/** A popcorn bucket, about 400 wide and 570 high; its rim sits at y = -110. */
export function popcorn(o = {}) {
	const clip = `bucket-${o.id ?? 'a'}`;
	let out = `<clipPath id="${clip}"><path d="M-180 -102 L180 -102 L130 268 Q128 282 114 282 L-114 282 Q-128 282 -130 268 Z"/></clipPath>`;
	if (o.kernels !== false) {
		out += group(
			[
				[-140, -126, 46],
				[140, -126, 46],
				[-82, -160, 54],
				[84, -158, 52],
				[0, -170, 58],
				[-114, -208, 42],
				[116, -204, 42],
				[-50, -236, 50],
				[54, -240, 50],
				[2, -290, 44]
			]
				.map(([x, y, r], i) => circle(x, y, r, i % 2 ? C.paper : '#fff6dc'))
				.join(''),
			{ stroke: '#e2c27a', sw: 6 }
		);
		out += [
			[-64, -148, 12],
			[18, -156, 14],
			[98, -142, 11],
			[-34, -226, 11],
			[70, -228, 12],
			[-128, -114, 10],
			[14, -282, 10]
		]
			.map(([x, y, r]) => circle(x, y, r, '#ffd879'))
			.join('');
	}
	out += `<g clip-path="url(#${clip})">`;
	out += rect(-212, -110, 424, 400, 0, C.cream);
	out += poly(
		[
			[-128.6, -102],
			[-77.1, -102],
			[-55.7, 282],
			[-92.9, 282]
		],
		C.red
	);
	out += poly(
		[
			[-25.7, -102],
			[25.7, -102],
			[18.6, 282],
			[-18.6, 282]
		],
		C.red
	);
	out += poly(
		[
			[77.1, -102],
			[128.6, -102],
			[92.9, 282],
			[55.7, 282]
		],
		C.red
	);
	if (o.band !== false) {
		out +=
			rect(-212, 28, 424, 112, 0, '#23342b') + rect(-212, 28, 424, 8, 0, '#000', { opacity: 0.25 });
	}
	out += poly(
		[
			[128, -110],
			[212, -110],
			[212, 290],
			[88, 290]
		],
		'#000',
		{ opacity: 0.12 }
	);
	out += '</g>';
	out +=
		rect(-194, -116, 388, 30, 12, '#ece5cf') +
		rect(-194, -94, 388, 8, 4, '#000', { opacity: 0.12 });
	return out;
}

/** A film reel, radius `r`. */
export function reel(r, o = {}) {
	const metal = linear('reel', [
		[0, '#eef3ef'],
		[1, '#9aa8a0']
	]);
	let out = circle(0, 0, r, o.color ?? metal, { stroke: '#5d6b63', sw: r * 0.05 });
	out += circle(0, 0, r * 0.82, 'none', { stroke: '#5d6b63', sw: r * 0.025, opacity: 0.5 });
	for (let i = 0; i < 5; i++) {
		const a = ((i * 72 - 90) * Math.PI) / 180;
		out += circle(Math.cos(a) * r * 0.5, Math.sin(a) * r * 0.5, r * 0.2, o.hole ?? '#23342b');
	}
	out += circle(0, 0, r * 0.13, '#5d6b63') + circle(0, 0, r * 0.06, '#eef3ef');
	return out;
}

/** A strip of film along the x axis, `len` long and `h` high. */
export function filmStrip(len, h, o = {}) {
	let out = rect(-len / 2, -h / 2, len, h, h * 0.08, o.color ?? '#23342b');
	const hole = h * 0.13;
	const step = h * 0.32;
	for (let x = -len / 2 + step * 0.6; x < len / 2 - hole; x += step) {
		out += rect(x, -h / 2 + h * 0.08, hole, hole, hole * 0.25, o.holes ?? C.cream);
		out += rect(x, h / 2 - h * 0.08 - hole, hole, hole, hole * 0.25, o.holes ?? C.cream);
	}
	const frames = o.frames ?? Math.max(1, Math.round(len / (h * 1.05)));
	const fw = len / frames;
	for (let i = 0; i < frames; i++) {
		out += rect(
			-len / 2 + i * fw + fw * 0.08,
			-h * 0.26,
			fw * 0.84,
			h * 0.52,
			h * 0.04,
			o.frame ?? '#3e7d6a'
		);
	}
	return out;
}

/** A clapperboard, about 300 wide. */
export function clapper(o = {}) {
	let out = rect(-150, -60, 300, 190, 16, '#23342b');
	out += rect(-130, -36, 260, 4, 2, '#ffffff', { opacity: 0.25 });
	out += line('M-120 30 L120 30 M-120 80 L120 80', '#ffffff', 5, { opacity: 0.35 });
	out += at(
		-150,
		-64,
		1,
		o.open ?? -16,
		rect(0, -52, 300, 52, 10, '#ffffff') +
			[0, 1, 2, 3, 4]
				.map((i) =>
					poly(
						[
							[i * 60 + 10, -52],
							[i * 60 + 40, -52],
							[i * 60 + 18, 0],
							[i * 60 - 12, 0]
						],
						'#23342b'
					)
				)
				.join('')
	);
	out += rect(-150, -66, 300, 26, 8, '#ffffff');
	out += [0, 1, 2, 3, 4]
		.map((i) =>
			poly(
				[
					[-140 + i * 60, -66],
					[-110 + i * 60, -66],
					[-128 + i * 60, -40],
					[-158 + i * 60, -40]
				],
				'#23342b'
			)
		)
		.join('');
	return out;
}

/** A ticket, `w` by `h`, notched at the sides. */
export function ticket(w, h, o = {}) {
	const n = h * 0.16;
	const fill = o.color ?? C.yellow;
	const d = `M${-w / 2} ${-h / 2} H${w / 2} V${-n} A${n} ${n} 0 0 0 ${w / 2} ${n} V${h / 2} H${-w / 2} V${n} A${n} ${n} 0 0 0 ${-w / 2} ${-n} Z`;
	let out = path(d, fill, { stroke: o.edge ?? 'rgba(0,0,0,0.18)', sw: 4 });
	out += line(`M${w * 0.18} ${-h / 2 + 14} L${w * 0.18} ${h / 2 - 14}`, o.ink ?? '#7a4d24', 4, {
		dash: '2 12',
		opacity: 0.6
	});
	if (o.star !== false)
		out += at(
			-w * 0.14,
			0,
			1,
			0,
			poly(starPoints(5, h * 0.2, 0.48), o.ink ?? '#b5651d', { opacity: 0.8 })
		);
	return out;
}

/** An old television, `w` by `h`, with `screen` drawn in its screen's middle. */
export function tv(w, h, o = {}) {
	const sw = w - 112;
	const sh = h - 48;
	const sx = -w / 2 + 24;
	const sy = -h / 2 + 24;
	const glow = radial('tvscreen', [
		[0, '#8ff0ff'],
		[0.7, '#35b7c9'],
		[1, '#1f7f8e']
	]);
	let out = '';
	if (o.antenna !== false) {
		out += line(
			`M0 ${-h / 2} L${-w * 0.22} ${-h / 2 - h * 0.42} M0 ${-h / 2} L${w * 0.18} ${-h / 2 - h * 0.46}`,
			'#9aa6a0',
			7
		);
		out +=
			circle(-w * 0.22, -h / 2 - h * 0.42, 9, C.red) +
			circle(w * 0.18, -h / 2 - h * 0.46, 9, C.red);
	}
	if (o.legs !== false) {
		out += line(
			`M${-w * 0.32} ${h / 2} L${-w * 0.38} ${h / 2 + 26} M${w * 0.32} ${h / 2} L${w * 0.38} ${h / 2 + 26}`,
			'#6b2a14',
			12
		);
	}
	out += rect(-w / 2, -h / 2, w, h, 30, o.body ?? '#c94a26');
	out += rect(-w / 2 + 8, -h / 2 + 8, w - 16, h - 16, 24, o.bodyLight ?? '#e4572e');
	out += rect(sx, sy, sw, sh, 26, '#23342b');
	out += rect(sx + 8, sy + 8, sw - 16, sh - 16, 20, o.off ? '#2e4538' : glow);
	if (!o.off)
		out += path(`M${sx + 26} ${sy + 22} Q${sx + 60} ${sy + 14} ${sx + 96} ${sy + 18}`, 'none', {
			stroke: '#ffffff',
			sw: 7,
			opacity: 0.45
		});
	const kx = w / 2 - 44;
	out += circle(kx, -h / 2 + 52, 16, '#23342b') + circle(kx, -h / 2 + 52, 6, C.cream);
	out += circle(kx, -h / 2 + 100, 12, '#23342b');
	out += line(
		`M${kx - 14} ${h / 2 - 50} L${kx + 14} ${h / 2 - 50} M${kx - 14} ${h / 2 - 34} L${kx + 14} ${h / 2 - 34}`,
		'#23342b',
		5
	);
	if (o.screen) out += at(sx + sw / 2, sy + sh / 2, 1, 0, o.screen);
	return out;
}

/** A remote, about 56 by 150. */
export function remote(o = {}) {
	return (
		rect(-28, -75, 56, 150, 20, '#23342b') +
		rect(-20, -67, 40, 134, 14, '#2e4538') +
		circle(0, -45, 11, o.button ?? C.red) +
		[
			[-11, -11],
			[11, -11],
			[-11, 11],
			[11, 11]
		]
			.map(([x, y]) => circle(x, y, 6, C.grey))
			.join('') +
		circle(0, 39, 7, C.grey)
	);
}

/** An armchair seen from the front, about 490 wide and 500 high, seat top at y = 60. */
export function armchair(o = {}) {
	const c = o.color ?? C.red;
	const deep = o.deep ?? C.redDeep;
	let out = rect(-190, 210, 36, 46, 8, '#6b2a14') + rect(154, 210, 36, 46, 8, '#6b2a14');
	out += rect(-186, -242, 372, 360, 80, o.back ?? '#c94a26') + rect(-156, -214, 312, 300, 60, deep);
	out +=
		circle(-82, -150, 9, '#8f3016') +
		circle(0, -160, 9, '#8f3016') +
		circle(82, -150, 9, '#8f3016');
	out += rect(-160, 64, 320, 104, 34, o.seat ?? '#ef6a40');
	return {
		back: out,
		front:
			rect(-246, -64, 118, 262, 56, c) +
			rect(128, -64, 118, 262, 56, c) +
			rect(-246, -64, 118, 40, 20, '#f07a50') +
			rect(128, -64, 118, 40, 20, '#f07a50') +
			rect(-212, 146, 424, 76, 30, deep)
	};
}

/** A sofa, about 560 wide; the seat is at y = 0. */
export function sofa(o = {}) {
	const c = o.color ?? '#3f7fe0';
	const deep = o.deep ?? '#2c5fb0';
	const light = o.light ?? '#6a9ff0';
	return {
		back:
			rect(-250, -150, 500, 190, 60, deep) +
			rect(-226, -128, 220, 150, 40, c) +
			rect(6, -128, 220, 150, 40, c),
		front:
			rect(-280, -40, 90, 150, 40, light) +
			rect(190, -40, 90, 150, 40, light) +
			rect(-200, 10, 400, 70, 26, c) +
			rect(-250, 70, 500, 60, 24, deep) +
			rect(-236, 128, 26, 30, 6, '#6b2a14') +
			rect(210, 128, 26, 30, 6, '#6b2a14')
	};
}

/** A projector on its side, about 360 wide, lens at (180, 40). */
export function projector() {
	let out = at(-70, -120, 1, 0, reel(80)) + at(80, -130, 1, 0, reel(95));
	out += rect(-160, -40, 320, 160, 28, '#3e4650') + rect(-150, -30, 300, 140, 22, '#5d6873');
	out +=
		rect(150, 10, 60, 70, 12, '#3e4650') +
		circle(214, 45, 34, '#23342b') +
		circle(214, 45, 22, '#cfe9ff');
	out += circle(-90, 40, 20, '#23342b') + circle(-30, 40, 12, '#23342b');
	out += rect(-130, 120, 30, 50, 6, '#23342b') + rect(100, 120, 30, 50, 6, '#23342b');
	return out;
}

/** A cone of light from (0, 0) spreading to the right, `len` long. */
export function beam(len, spread, o = {}) {
	const fill = linear(
		`beam-${o.id ?? 'a'}`,
		[
			[0, o.color ?? '#fff7cc', 0.85],
			[1, o.color ?? '#fff7cc', 0]
		],
		{ x1: 0, y1: 0, x2: 1, y2: 0 }
	);
	return poly(
		[
			[0, -12],
			[len, -spread],
			[len, spread],
			[0, 12]
		],
		fill
	);
}

/** A spotlight from above: a cone down to a pool of light. */
export function spotlight(top, bottom, w, o = {}) {
	const fill = linear(`spot-${o.id ?? 'a'}`, [
		[0, '#fff7cc', 0.05],
		[1, '#fff7cc', 0.5]
	]);
	return (
		poly(
			[
				[-30, top],
				[30, top],
				[w, bottom],
				[-w, bottom]
			],
			fill
		) + ellipse(0, bottom, w, w * 0.18, '#fff7cc', { opacity: 0.35 })
	);
}

/** A red carpet running into the distance, bottom width `w`. */
export function carpet(w, h) {
	return (
		poly(
			[
				[-w * 0.22, -h],
				[w * 0.22, -h],
				[w / 2, 0],
				[-w / 2, 0]
			],
			'#c62a3c'
		) +
		poly(
			[
				[-w * 0.22, -h],
				[-w * 0.19, -h],
				[-w * 0.44, 0],
				[-w / 2, 0]
			],
			C.gold
		) +
		poly(
			[
				[w * 0.22, -h],
				[w * 0.19, -h],
				[w * 0.44, 0],
				[w / 2, 0]
			],
			C.gold
		)
	);
}

/** A cinema seat seen from the front, about 300 wide. */
export function seat(o = {}) {
	const c = o.color ?? '#c62a3c';
	return {
		back: rect(-120, -210, 240, 230, 50, '#8e1730') + rect(-104, -194, 208, 200, 40, c),
		front:
			rect(-150, -40, 50, 150, 22, '#8e1730') +
			rect(100, -40, 50, 150, 22, '#8e1730') +
			rect(-110, 20, 220, 70, 24, c) +
			rect(-110, 80, 220, 30, 12, '#8e1730')
	};
}

// --- shapes and things --------------------------------------------------------

export function star(r, color = C.yellow, o = {}) {
	return poly(starPoints(o.points ?? 5, r, o.k ?? 0.48), color, {
		stroke: o.edge ?? color,
		sw: r * 0.14
	});
}

/** A heart, about 110 wide. */
export function heart(color = C.red) {
	return path(
		'M0 38 C-12 28 -56 2 -56 -24 C-56 -48 -30 -60 0 -34 C30 -60 56 -48 56 -24 C56 2 12 28 0 38 Z',
		color
	);
}

export function sparkles(points, color = '#fff7cc') {
	return points.map(([x, y, r]) => glint(x, y, r, color)).join('');
}

/** A trophy cup, about 260 wide and 330 high, its rim at y = -150. */
export function trophy(o = {}) {
	const g = linear(
		'trophy',
		[
			[0, '#fff3b8'],
			[0.45, C.gold],
			[1, C.goldDeep]
		],
		{ x1: 0, y1: 0, x2: 1, y2: 0.6 }
	);
	let out =
		line('M-96 -110 C-170 -110 -160 -10 -70 -6', C.goldDeep, 20) +
		line('M96 -110 C170 -110 160 -10 70 -6', C.goldDeep, 20);
	out += path('M-110 -150 L110 -150 Q110 40 0 50 Q-110 40 -110 -150 Z', o.cup ?? g);
	out +=
		rect(-22, 44, 44, 60, 6, C.goldDeep) +
		rect(-80, 100, 160, 36, 10, '#7a5410') +
		rect(-96, 132, 192, 34, 10, '#5c3f04');
	out += ellipse(0, -150, 110, 16, '#fff3b8', { opacity: 0.6 });
	return out;
}

/** A crown on its own, about 220 wide. */
export function crown() {
	return (
		path('M-110 40 L-120 -60 L-60 -10 L0 -90 L60 -10 L120 -60 L110 40 Z', C.gold, {
			stroke: C.goldDeep,
			sw: 8
		}) +
		rect(-112, 30, 224, 34, 10, C.goldDeep) +
		circle(0, -10, 16, C.red) +
		circle(-60, 14, 11, C.teal) +
		circle(60, 14, 11, C.teal) +
		circle(0, -92, 12, '#fff3b8') +
		circle(-120, -62, 10, '#fff3b8') +
		circle(120, -62, 10, '#fff3b8')
	);
}

/** The streak flame, about 380 wide and 600 high, from y = -340 to 250. */
export function flame(o = {}) {
	const outer = linear(`flame-${o.id ?? 'a'}`, [
		[0, o.top ?? '#ffb14a'],
		[0.55, o.mid ?? '#f27a33'],
		[1, o.bottom ?? '#d9482a']
	]);
	const inner = linear(`core-${o.id ?? 'a'}`, [
		[0, '#fff3c4'],
		[1, '#ffcf5a']
	]);
	return (
		path(
			'M0 -354 C36 -268 110 -222 148 -142 C184 -74 192 -18 188 36 C180 156 100 246 0 246 C-100 246 -184 156 -188 38 C-191 -40 -156 -108 -108 -160 C-106 -112 -90 -74 -58 -58 C-74 -146 -46 -264 0 -354 Z',
			outer
		) +
		path(
			'M-108 -160 C-142 -110 -160 -40 -152 40 C-146 90 -126 130 -102 156 C-132 90 -136 0 -108 -160 Z',
			'#ffd27a',
			{ opacity: 0.45 }
		) +
		(o.core === false
			? ''
			: path(
					'M0 -142 C32 -78 104 -34 104 62 C104 146 56 200 0 200 C-56 200 -104 146 -104 68 C-104 2 -60 -38 -46 -90 C-32 -58 -20 -46 -8 -42 C-14 -72 -12 -104 0 -142 Z',
					inner
				))
	);
}

/** A clock face, radius `r`, hands at `h` hours and `m` minutes. */
export function clock(r, o = {}) {
	let out = circle(0, 0, r, o.face ?? C.cream, { stroke: o.rim ?? '#23342b', sw: r * 0.1 });
	for (let i = 0; i < 12; i++) {
		const a = (i * 30 * Math.PI) / 180;
		const inner = i % 3 === 0 ? 0.72 : 0.8;
		out += line(
			`M${f(Math.sin(a) * r * inner)} ${f(-Math.cos(a) * r * inner)} L${f(Math.sin(a) * r * 0.88)} ${f(-Math.cos(a) * r * 0.88)}`,
			'#23342b',
			r * (i % 3 === 0 ? 0.06 : 0.035)
		);
	}
	if (o.hands !== false) {
		const h = (((o.h ?? 10) % 12) * 30 + (o.m ?? 10) * 0.5) * (Math.PI / 180);
		const m = (o.m ?? 10) * 6 * (Math.PI / 180);
		out += line(
			`M0 0 L${f(Math.sin(h) * r * 0.48)} ${f(-Math.cos(h) * r * 0.48)}`,
			o.hand ?? '#23342b',
			r * 0.08
		);
		out += line(
			`M0 0 L${f(Math.sin(m) * r * 0.7)} ${f(-Math.cos(m) * r * 0.7)}`,
			o.hand ?? '#23342b',
			r * 0.06
		);
		out += circle(0, 0, r * 0.07, o.hand ?? C.red);
	}
	return out;
}

/** A calendar page, `w` by `h`, with `inside` drawn in the middle of its page. */
export function calendar(w, h, o = {}) {
	let out = rect(-w / 2, -h / 2 + 10, w, h, 22, '#d8d2c0');
	out += rect(-w / 2, -h / 2, w, h, 22, C.paper);
	out += path(
		`M${-w / 2} ${-h / 2 + 22} Q${-w / 2} ${-h / 2} ${-w / 2 + 22} ${-h / 2} H${w / 2 - 22} Q${w / 2} ${-h / 2} ${w / 2} ${-h / 2 + 22} V${-h / 2 + h * 0.26} H${-w / 2} Z`,
		o.head ?? C.red
	);
	out += circle(-w * 0.25, -h / 2, 12, '#23342b') + circle(w * 0.25, -h / 2, 12, '#23342b');
	out +=
		rect(-w * 0.25 - 5, -h / 2 - 26, 10, 34, 5, '#9aa6a0') +
		rect(w * 0.25 - 5, -h / 2 - 26, 10, 34, 5, '#9aa6a0');
	if (o.inside) out += at(0, h * 0.13, 1, 0, o.inside);
	return out;
}

/** A crescent moon, radius `r`, its hollow to the right. */
export function moon(r, color = '#fff3c4') {
	return path(`M0 ${-r} A${r} ${r} 0 1 0 0 ${r} A${r * 0.78} ${r} 0 1 1 0 ${-r} Z`, color);
}

export function sun(r, color = C.yellow) {
	let out = '';
	for (let i = 0; i < 12; i++) {
		const a = (i * 30 * Math.PI) / 180;
		out += line(
			`M${f(Math.cos(a) * r * 1.2)} ${f(Math.sin(a) * r * 1.2)} L${f(Math.cos(a) * r * 1.55)} ${f(Math.sin(a) * r * 1.55)}`,
			color,
			r * 0.14
		);
	}
	return out + circle(0, 0, r, color);
}

/** A laurel branch curving up the left (side -1) or right (side 1). */
export function laurel(side, r, o = {}) {
	const color = o.color ?? '#7fcf6f';
	let out = '';
	const n = o.leaves ?? 7;
	const from = 100;
	const to = 230;
	const pts = [];
	for (let i = 0; i <= 40; i++) {
		const a = ((from + ((to - from) * i) / 40) * Math.PI) / 180;
		pts.push([Math.cos(a) * r * -side, Math.sin(a) * r]);
	}
	out += line(
		`M${pts.map(([x, y]) => `${f(x)} ${f(y)}`).join(' L')}`,
		o.stem ?? '#4c9a54',
		r * 0.03
	);
	for (let i = 0; i < n; i++) {
		const t = from + ((to - from) * (i + 0.6)) / n;
		const a = (t * Math.PI) / 180;
		const x = Math.cos(a) * r * -side;
		const y = Math.sin(a) * r;
		const tangent = (Math.atan2(Math.cos(a) * r, Math.sin(a) * r * side) * 180) / Math.PI;
		out += at(x, y, 1, -tangent + side * 40, ellipse(r * 0.08, 0, r * 0.11, r * 0.045, color));
		out += at(
			x,
			y,
			1,
			-tangent - side * 40 + 180,
			ellipse(r * 0.08, 0, r * 0.1, r * 0.04, o.dark ?? '#5fb85a')
		);
	}
	return out;
}

/** An open book, about 300 wide, spine at x = 0. */
export function openBook(o = {}) {
	return (
		path('M0 -60 Q-70 -90 -150 -70 L-150 70 Q-70 50 0 80 Z', o.left ?? C.paper, {
			stroke: '#cfc8b6',
			sw: 4
		}) +
		path('M0 -60 Q70 -90 150 -70 L150 70 Q70 50 0 80 Z', o.right ?? '#fff6dc', {
			stroke: '#cfc8b6',
			sw: 4
		}) +
		line(
			'M-120 -40 Q-70 -56 -24 -36 M-120 -10 Q-70 -26 -24 -6 M-120 20 Q-70 4 -24 24',
			'#b9b2a0',
			5
		) +
		line('M24 -36 Q70 -56 120 -40 M24 -6 Q70 -26 120 -10 M24 24 Q70 4 120 20', '#b9b2a0', 5) +
		path(
			'M-160 -64 L-160 80 Q-80 60 0 92 Q80 60 160 80 L160 -64 L150 -64 L150 70 Q70 50 0 80 Q-70 50 -150 70 L-150 -64 Z',
			o.cover ?? '#c94a26'
		)
	);
}

/** A closed book lying flat, `w` wide, spine facing you. */
export function bookSpine(w, h, color, o = {}) {
	return (
		rect(-w / 2, -h / 2, w, h, h * 0.18, color) +
		rect(-w / 2 + w * 0.08, -h / 2, w * 0.05, h, 0, '#000', { opacity: 0.18 }) +
		rect(w / 2 - w * 0.13, -h / 2, w * 0.05, h, 0, '#000', { opacity: 0.18 }) +
		(o.label === false
			? ''
			: rect(-w * 0.18, -h * 0.22, w * 0.36, h * 0.44, 4, '#ffffff', { opacity: 0.7 }))
	);
}

/** A big tick, about 200 wide. */
export function check(color = '#8af0b4', w = 34) {
	return line('M-80 0 L-24 56 L90 -64', color, w);
}

/** A poster, `w` by `h`, of a little landscape in `color`. */
export function poster(w, h, color = C.blue, o = {}) {
	return (
		rect(-w / 2 - 6, -h / 2 - 6, w + 12, h + 12, 8, o.frame ?? '#f5f1e4') +
		rect(-w / 2, -h / 2, w, h, 4, color) +
		circle(w * 0.18, -h * 0.2, w * 0.14, '#fff3c4', { opacity: 0.85 }) +
		poly(
			[
				[-w / 2, h / 2],
				[-w * 0.1, -h * 0.05],
				[w * 0.15, h * 0.2],
				[w * 0.3, h * 0.05],
				[w / 2, h / 2]
			],
			'#000',
			{ opacity: 0.22 }
		)
	);
}

export const POSTER_COLOURS = [
	'#3f7fe0',
	'#e4572e',
	'#8f5bd6',
	'#35b7c9',
	'#f2c444',
	'#ff8fb1',
	'#4ea85f'
];

/** An hourglass, about 180 wide and 300 high, with film for sand. */
export function hourglass(o = {}) {
	const glass = '#cfe9ff';
	let out = rect(-100, -160, 200, 26, 10, '#8c5225') + rect(-100, 134, 200, 26, 10, '#8c5225');
	out += path(
		'M-80 -134 L80 -134 Q80 -40 12 0 Q80 40 80 134 L-80 134 Q-80 40 -12 0 Q-80 -40 -80 -134 Z',
		glass,
		{ opacity: 0.55 }
	);
	out += path('M-56 -96 L56 -96 Q50 -40 6 -6 L-6 -6 Q-50 -40 -56 -96 Z', o.sand ?? '#23342b');
	out += path('M-70 130 Q-60 60 0 50 Q60 60 70 130 Z', o.sand ?? '#23342b');
	if (o.film !== false) {
		for (let y = -86; y < -20; y += 18) out += rect(-40 + (y + 86) * 0.4, y, 10, 8, 2, C.cream);
		for (let x = -50; x <= 50; x += 22) out += rect(x - 5, 100, 10, 8, 2, C.cream);
	}
	out += line('M0 -6 L0 50', o.sand ?? '#23342b', 6);
	out += line('M-90 -134 L-90 134 M90 -134 L90 134', '#8c5225', 12);
	return out;
}

/** A waving flag on a pole whose foot is at (0, 0), `h` high. */
export function flag(h, o = {}) {
	let out = line(`M0 0 L0 ${-h}`, '#9aa6a0', 12) + circle(0, -h - 6, 12, C.gold);
	const fw = h * 0.62;
	const fh = h * 0.4;
	const d = `M0 ${-h} Q${fw * 0.5} ${-h - fh * 0.25} ${fw} ${-h} L${fw} ${-h + fh} Q${fw * 0.5} ${-h + fh * 0.75} 0 ${-h + fh} Z`;
	if (o.checkered) {
		const id = `chk-${o.id ?? 'a'}`;
		out +=
			`<clipPath id="${id}"><path d="${d}"/></clipPath><g clip-path="url(#${id})">` +
			rect(0, -h - fh, fw, fh * 2, 0, '#ffffff');
		const n = 5;
		for (let i = 0; i < n; i++)
			for (let j = 0; j < 4; j++)
				if ((i + j) % 2 === 0)
					out += rect(
						(i * fw) / n,
						-h - fh * 0.3 + (j * fh * 1.4) / 4,
						fw / n,
						(fh * 1.4) / 4,
						0,
						'#23342b'
					);
		out += '</g>';
	} else {
		out += path(d, o.color ?? C.red);
		if (o.mark) out += at(fw * 0.45, -h + fh * 0.45, 1, 0, o.mark);
	}
	return out;
}

/** A pumpkin, about 300 wide, its face an old television screen. */
export function pumpkin() {
	let out = line('M0 -110 Q10 -150 34 -160', '#3f8f52', 18);
	out += ellipse(-80, 0, 90, 110, '#d9601e') + ellipse(80, 0, 90, 110, '#d9601e');
	out +=
		ellipse(0, 0, 100, 118, '#f27a33') +
		ellipse(-40, 0, 60, 112, '#f58a40') +
		ellipse(40, 0, 60, 112, '#f58a40');
	out += ellipse(0, 0, 40, 116, '#f27a33');
	out +=
		rect(-74, -46, 148, 104, 22, '#3a1606') +
		rect(-60, -34, 120, 80, 14, '#ffc94a', { opacity: 0.95 });
	out +=
		poly(
			[
				[-36, -14],
				[-14, -14],
				[-25, 4]
			],
			'#3a1606'
		) +
		poly(
			[
				[14, -14],
				[36, -14],
				[25, 4]
			],
			'#3a1606'
		);
	out += path('M-34 18 L-20 28 L-8 18 L4 28 L16 18 L28 28 L36 18 Q0 46 -34 18 Z', '#3a1606');
	return out;
}

/** A firework burst, radius `r`. */
export function burst(r, color, o = {}) {
	let out = '';
	const n = o.rays ?? 12;
	for (let i = 0; i < n; i++) {
		const a = ((i * 360) / n) * (Math.PI / 180);
		out += line(
			`M${f(Math.cos(a) * r * 0.3)} ${f(Math.sin(a) * r * 0.3)} L${f(Math.cos(a) * r * 0.85)} ${f(Math.sin(a) * r * 0.85)}`,
			color,
			r * 0.07
		);
		out += circle(Math.cos(a) * r, Math.sin(a) * r, r * 0.07, color);
	}
	return out;
}

/** A snail with a shell like a film reel, about 260 wide. */
export function snail(o = {}) {
	let out = path(
		'M-130 50 Q-120 20 -60 26 L100 26 Q150 26 150 -20 Q150 -50 130 -54 Q114 -20 100 0 L-60 0 Q-140 0 -130 50 Z',
		'#e9c89a'
	);
	out +=
		line('M118 -50 L108 -100 M136 -50 L148 -96', '#c9a87a', 6) +
		circle(108, -102, 8, '#c9a87a') +
		circle(148, -98, 8, '#c9a87a');
	out += circle(132, -24, 5, '#23342b');
	out += at(
		-10,
		-50,
		1,
		0,
		circle(0, 0, 86, '#b5651d') +
			line(
				'M0 0 C20 -10 18 -40 -6 -42 C-40 -44 -50 -6 -30 20 C-6 50 48 44 60 6 C74 -40 40 -76 -2 -76',
				'#7a3e10',
				12
			)
	);
	if (o.ticket) out += at(-20, -150, 0.4, -14, ticket(220, 110));
	return out;
}

/** A cut gem, about 200 wide. */
export function gem(color = '#8af0ff', o = {}) {
	return (
		poly(
			[
				[-100, -30],
				[-50, -80],
				[50, -80],
				[100, -30],
				[0, 90]
			],
			color
		) +
		poly(
			[
				[-100, -30],
				[-50, -80],
				[-26, -30]
			],
			'#ffffff',
			{ opacity: 0.45 }
		) +
		poly(
			[
				[-26, -30],
				[0, -80],
				[26, -30]
			],
			'#ffffff',
			{ opacity: 0.25 }
		) +
		poly(
			[
				[-100, -30],
				[100, -30],
				[0, 90]
			],
			'#000',
			{ opacity: 0.12 }
		) +
		poly(
			[
				[-26, -30],
				[26, -30],
				[0, 90]
			],
			'#ffffff',
			{ opacity: 0.18 }
		) +
		line('M-100 -30 L100 -30', o.edge ?? '#ffffff', 4, { opacity: 0.5 })
	);
}

/** A magnifying glass, lens radius `r`, handle down to the right. */
export function magnifier(r, o = {}) {
	return (
		line(`M${f(r * 0.7)} ${f(r * 0.7)} L${f(r * 1.6)} ${f(r * 1.6)}`, '#6b2a14', r * 0.28) +
		circle(0, 0, r, '#cfe9ff', { opacity: o.clear ? 0.25 : 0.35 }) +
		circle(0, 0, r, 'none', { stroke: '#3e4650', sw: r * 0.14 }) +
		path(
			`M${f(-r * 0.6)} ${f(-r * 0.3)} A${r * 0.7} ${r * 0.7} 0 0 1 ${f(-r * 0.2)} ${f(-r * 0.65)}`,
			'none',
			{ stroke: '#ffffff', sw: r * 0.08, opacity: 0.7 }
		)
	);
}

export function mic() {
	return (
		line('M0 60 L0 160 M-60 160 L60 160', '#9aa6a0', 14) +
		path('M-60 0 Q-60 70 0 70 Q60 70 60 0', 'none', { stroke: '#9aa6a0', sw: 12 }) +
		rect(-40, -110, 80, 160, 40, '#3e4650') +
		[-80, -50, -20, 10].map((y) => line(`M-28 ${y} L28 ${y}`, '#7b8590', 6)).join('')
	);
}

/** A pencil pointing down-left, about 320 long. */
export function pencil() {
	return (
		rect(-110, -24, 240, 48, 6, C.yellow) +
		rect(-110, -24, 240, 16, 0, '#ffe07a') +
		rect(130, -24, 30, 48, 4, '#c9cfd4') +
		rect(160, -24, 30, 48, 10, C.pink) +
		poly(
			[
				[-110, -24],
				[-170, 0],
				[-110, 24]
			],
			'#f2d4a6'
		) +
		poly(
			[
				[-148, -9],
				[-170, 0],
				[-148, 9]
			],
			'#23342b'
		)
	);
}

/** A laughing theatre mask, about 220 wide. */
export function mask(color = C.yellow) {
	return (
		path('M-110 -90 Q0 -130 110 -90 Q120 40 0 110 Q-120 40 -110 -90 Z', color, {
			stroke: '#b58a1a',
			sw: 6
		}) +
		line('M-70 -30 Q-46 -54 -22 -30 M22 -30 Q46 -54 70 -30', '#3a2a06', 12) +
		path('M-60 20 Q0 70 60 20 Q50 80 0 84 Q-50 80 -60 20 Z', '#3a2a06') +
		ellipse(-76, 4, 16, 10, '#f4738f', { opacity: 0.6 }) +
		ellipse(76, 4, 16, 10, '#f4738f', { opacity: 0.6 })
	);
}

export function chili() {
	return (
		path('M-120 -40 Q-60 -70 40 -40 Q120 -14 130 70 Q90 10 20 6 Q-80 0 -120 -40 Z', '#d9302a') +
		path('M-90 -42 Q-30 -56 30 -36', 'none', { stroke: '#ffffff', sw: 8, opacity: 0.4 }) +
		path('M-122 -40 Q-150 -60 -150 -94 Q-130 -66 -108 -62 Z', '#3f8f52') +
		ellipse(-118, -48, 22, 14, '#4ea85f')
	);
}

/** A birthday cake, about 300 wide, with `candles` frog candles. */
export function cake(candles = 1) {
	let out = rect(-150, 10, 300, 110, 22, '#f4d6b0') + rect(-150, 10, 300, 34, 16, C.pink);
	out += path(
		'M-150 40 Q-125 70 -100 40 Q-75 70 -50 40 Q-25 70 0 40 Q25 70 50 40 Q75 70 100 40 Q125 70 150 40 L150 30 L-150 30 Z',
		C.pink
	);
	out += rect(-100, -70, 200, 90, 18, '#f9e4c8') + rect(-100, -70, 200, 26, 12, '#ffffff');
	out += ellipse(0, 122, 180, 16, '#d8d2c0');
	const xs =
		candles === 1
			? [0]
			: Array.from({ length: candles }, (_, i) => -60 + (120 * i) / (candles - 1));
	for (const x of xs) {
		out +=
			rect(x - 9, -130, 18, 62, 6, C.skin) +
			ellipse(x, -146, 9, 16, '#ffb14a') +
			ellipse(x, -142, 4.5, 8, '#fff3c4');
		out +=
			circle(x - 5, -116, 3.4, '#ffffff') +
			circle(x + 5, -116, 3.4, '#ffffff') +
			circle(x - 5, -116, 1.6, '#11231a') +
			circle(x + 5, -116, 1.6, '#11231a');
	}
	return out;
}

export function mug(color = '#3f7fe0') {
	return (
		path('M110 -30 Q170 -30 170 20 Q170 70 104 70', 'none', { stroke: color, sw: 26 }) +
		rect(-110, -70, 230, 200, 40, color) +
		rect(-110, -70, 230, 30, 16, '#ffffff', { opacity: 0.15 }) +
		at(4, 30, 1, 0, heart('#ffffff'), '')
	);
}

/** A bookshelf of posters and boxes, about 380 wide and 360 high. */
export function shelf(o = {}) {
	let out = rect(-190, -180, 380, 360, 16, '#8c5225') + rect(-170, -160, 340, 320, 10, '#5e3416');
	out += rect(-170, -10, 340, 18, 4, '#8c5225');
	const colours = o.colours ?? POSTER_COLOURS;
	let x = -156;
	[44, 36, 52, 30, 48, 40].forEach((w, i) => {
		out += rect(x, -140 + (i % 2) * 14, w, 124 - (i % 2) * 14, 6, colours[i % colours.length]);
		x += w + 8;
	});
	x = -156;
	[60, 44, 70, 52, 66].forEach((w, i) => {
		out += rect(x, 24 + (i % 3) * 10, w, 122 - (i % 3) * 10, 6, colours[(i + 3) % colours.length]);
		x += w + 8;
	});
	return out;
}

/** A studio building with a film-strip roof, about 360 wide. */
export function studio() {
	let out = rect(-170, -40, 340, 220, 10, '#d8d2c0') + rect(-170, -40, 340, 24, 6, '#bdb6a2');
	for (let i = 0; i < 4; i++)
		out += rect(-140 + i * 76, 10, 48, 56, 6, '#35b7c9', { opacity: 0.8 });
	out += rect(-40, 100, 80, 80, 8, '#8c5225');
	out += at(0, -90, 1, 0, filmStrip(400, 96, { frames: 4 }));
	return out;
}

/** A director's chair, about 300 wide, with a star on its back. */
export function directorChair() {
	return (
		line('M-110 40 L110 190 M110 40 L-110 190', '#8c5225', 18) +
		line('M-120 -150 L-120 190 M120 -150 L120 190', '#a8642e', 18) +
		rect(-130, -150, 260, 90, 10, '#c62a3c') +
		at(0, -105, 1, 0, poly(starPoints(5, 34), C.yellow)) +
		rect(-130, 30, 260, 30, 8, '#c62a3c') +
		line('M-140 0 L-110 0 M110 0 L140 0', '#a8642e', 16)
	);
}

/** A jigsaw piece, about 220 wide, with a play mark on it. */
export function puzzle(color = '#4b86e6') {
	return (
		path(
			'M-100 -100 L-30 -100 Q-30 -150 0 -150 Q30 -150 30 -100 L100 -100 L100 -30 Q150 -30 150 0 Q150 30 100 30 L100 100 L-100 100 L-100 30 Q-60 30 -60 0 Q-60 -30 -100 -30 Z',
			color,
			{ stroke: '#1a3a86', sw: 8 }
		) +
		poly(
			[
				[-28, -40],
				[46, 0],
				[-28, 40]
			],
			'#ffffff'
		)
	);
}

/** A ship's wheel, radius `r`, wrapped in film. */
export function shipWheel(r) {
	let out = '';
	for (let i = 0; i < 8; i++) {
		const a = (i * 45 * Math.PI) / 180;
		out += line(
			`M0 0 L${f(Math.cos(a) * r * 1.22)} ${f(Math.sin(a) * r * 1.22)}`,
			'#8c5225',
			r * 0.09
		);
		out += circle(Math.cos(a) * r * 1.25, Math.sin(a) * r * 1.25, r * 0.1, '#a8642e');
	}
	out += circle(0, 0, r, 'none', { stroke: '#a8642e', sw: r * 0.16 });
	out += circle(0, 0, r * 0.22, '#a8642e') + circle(0, 0, r * 0.1, C.gold);
	return out;
}

/** A stopwatch, radius `r`. */
export function stopwatch(r) {
	return (
		rect(-r * 0.18, -r * 1.32, r * 0.36, r * 0.3, 6, '#9aa6a0') +
		at(r * 0.82, -r * 0.82, 1, 45, rect(-r * 0.1, -r * 0.16, r * 0.2, r * 0.22, 4, '#9aa6a0')) +
		circle(0, 0, r, '#e4572e') +
		clock(r * 0.8, { h: 0, m: 0, hands: false, rim: '#c94a26' }) +
		line(`M0 0 L${f(r * 0.38)} ${f(-r * 0.52)}`, '#23342b', r * 0.07) +
		circle(0, 0, r * 0.07, '#23342b')
	);
}

/** A running shoe, about 300 wide, toe to the right. */
export function shoe(color = '#3f7fe0') {
	return (
		path(
			'M-140 40 L-140 -40 Q-130 -70 -90 -60 Q-40 -46 0 -40 Q40 -10 100 0 Q150 10 150 40 Z',
			color
		) +
		rect(-150, 36, 310, 30, 14, '#ffffff') +
		line('M-60 -40 L-40 -12 M-34 -36 L-14 -10 M-8 -30 L10 -6', '#ffffff', 8) +
		path('M60 -6 Q100 6 130 22', 'none', { stroke: '#ffffff', sw: 8, opacity: 0.6 })
	);
}

export function telescope() {
	return (
		line('M0 40 L-60 170 M0 40 L60 170 M0 40 L0 170', '#8c5225', 12) +
		at(
			0,
			20,
			1,
			-28,
			rect(-140, -30, 230, 60, 16, '#3e4650') +
				rect(80, -38, 60, 76, 14, '#5d6873') +
				rect(-170, -20, 40, 40, 8, '#23342b')
		)
	);
}

export function gavel() {
	return (
		at(
			30,
			-30,
			1,
			-35,
			rect(-14, 0, 28, 190, 10, '#8c5225') +
				rect(-80, -50, 160, 64, 16, '#a8642e') +
				rect(-90, -46, 18, 56, 8, '#7a3e10') +
				rect(72, -46, 18, 56, 8, '#7a3e10')
		) + rect(-140, 130, 200, 40, 12, '#7a3e10')
	);
}

export function cushion(color = C.pink) {
	return (
		path('M-120 -110 Q0 -90 120 -110 Q100 0 120 110 Q0 90 -120 110 Q-100 0 -120 -110 Z', color) +
		circle(0, 0, 12, '#000', { opacity: 0.15 }) +
		line('M-60 -60 Q0 -40 60 -60', '#ffffff', 6, { opacity: 0.3 })
	);
}

/** A treasure chest spilling posters, about 380 wide. */
export function chest() {
	let out = '';
	POSTER_COLOURS.slice(0, 5).forEach((c, i) => {
		out += at(-120 + i * 60, -110 - (i % 2) * 30, 1, -20 + i * 10, poster(64, 90, c));
	});
	out += rect(-180, -60, 360, 200, 20, '#8c5225') + rect(-180, -60, 360, 40, 14, '#a8642e');
	out +=
		rect(-180, -10, 360, 18, 0, C.gold) +
		rect(-30, -30, 60, 70, 10, C.gold) +
		circle(0, 4, 10, '#5c3f04');
	out += line('M-120 -60 L-120 140 M120 -60 L120 140', C.gold, 14);
	out += path(
		'M-190 -60 Q-180 -140 0 -150 Q180 -140 190 -60 L170 -64 Q160 -120 0 -128 Q-160 -120 -170 -64 Z',
		'#a8642e'
	);
	return out;
}

/** A stack of rating cards, about 260 wide, the top one ticked. */
export function cards() {
	return (
		at(16, 22, 1, 10, rect(-120, -150, 240, 300, 22, '#b9b2a0')) +
		at(8, 10, 1, 4, rect(-120, -150, 240, 300, 22, '#d8d2c0')) +
		rect(-120, -150, 240, 300, 22, C.paper, { stroke: '#cfc8b6', sw: 4 }) +
		at(0, -50, 1, 0, star(46, C.yellow)) +
		at(0, 70, 0.6, 0, check('#4ea85f'))
	);
}

/** An antenna tower sending out waves. */
export function tower() {
	let out = line('M-60 180 L0 -140 L60 180', '#9aa6a0', 12);
	for (let y = -80; y < 180; y += 60) {
		const w = ((y + 140) / 320) * 60;
		out += line(`M${-w} ${y} L${w} ${y}`, '#9aa6a0', 8);
		out += line(`M${-w} ${y} L${w * 0.8} ${y + 52}`, '#9aa6a0', 5);
	}
	out += circle(0, -150, 16, C.red);
	for (const r of [60, 100, 140]) {
		out += path(
			`M${-r * 0.6} ${-150 - r * 0.6} A${r} ${r} 0 0 1 ${r * 0.6} ${-150 - r * 0.6}`,
			'none',
			{ stroke: '#8ff0ff', sw: 10, opacity: 1.1 - r / 200 }
		);
	}
	return out;
}

/** A throne of televisions, about 420 wide; the seat is at y = 40. */
export function throne() {
	let out = rect(-200, -260, 400, 300, 40, '#8e1730') + rect(-170, -230, 340, 260, 30, '#c62a3c');
	for (const [x, y] of [
		[-120, -170],
		[0, -190],
		[120, -170],
		[-60, -90],
		[60, -90]
	])
		out += at(x, y, 0.22, 0, tv(380, 280, { antenna: false, legs: false }));
	out += rect(-210, 30, 420, 80, 24, C.gold) + rect(-180, 100, 360, 90, 20, '#8e1730');
	return out;
}

/** Snow lying on the ground, `w` wide. */
export function snow(w) {
	return path(
		`M${-w / 2} 40 Q${-w * 0.35} -30 ${-w * 0.15} 0 Q0 -40 ${w * 0.18} -4 Q${w * 0.36} -36 ${w / 2} 40 Z`,
		'#eef6ff'
	);
}

/** Two arrows pointing apart, about 300 wide. */
export function apart(color = C.red) {
	return (
		line('M-30 0 L-130 0', color, 22) +
		poly(
			[
				[-170, 0],
				[-110, -44],
				[-110, 44]
			],
			color
		) +
		line('M30 0 L130 0', '#35b7c9', 22) +
		poly(
			[
				[170, 0],
				[110, -44],
				[110, 44]
			],
			'#35b7c9'
		)
	);
}

/** A running track loop, the infinity sign drawn as film. */
export function infinity(w, color = '#23342b') {
	const d = `M0 0 C${w * 0.15} ${-w * 0.3} ${w * 0.5} ${-w * 0.3} ${w * 0.5} 0 C${w * 0.5} ${w * 0.3} ${w * 0.15} ${w * 0.3} 0 0 C${-w * 0.15} ${-w * 0.3} ${-w * 0.5} ${-w * 0.3} ${-w * 0.5} 0 C${-w * 0.5} ${w * 0.3} ${-w * 0.15} ${w * 0.3} 0 0 Z`;
	return line(d, color, w * 0.14) + line(d, C.cream, w * 0.03, { dash: `${w * 0.03} ${w * 0.05}` });
}

/** A paper plane, about 200 wide, flying right. */
export function plane(color = '#ffffff') {
	return (
		poly(
			[
				[-100, 10],
				[100, -40],
				[-30, 40]
			],
			color
		) +
		poly(
			[
				[-30, 40],
				[100, -40],
				[-10, 70]
			],
			'#d8e6ef'
		) +
		poly(
			[
				[-100, 10],
				[-30, 40],
				[-60, 34]
			],
			'#b9cad6'
		)
	);
}

/** A curved arrow going round, radius `r`. */
export function loopArrow(r, color = '#8af0b4') {
	return (
		path(`M${-r} 0 A${r} ${r} 0 1 1 0 ${r}`, 'none', { stroke: color, sw: r * 0.2 }) +
		poly(
			[
				[-r * 1.35, -r * 0.05],
				[-r * 0.65, -r * 0.05],
				[-r, r * 0.45]
			],
			color
		)
	);
}

/** A trail of flying hearts or petals: `items` of [x, y, scale, turn]. */
export function petals(items, color = '#ffc4d8') {
	return items
		.map(([x, y, s, r]) =>
			at(x, y, s, r, ellipse(0, 0, 14, 8, color) + ellipse(-6, 0, 6, 3, '#ff9fbf'))
		)
		.join('');
}

/** A cherry-blossom flower. */
export function blossom(r = 30) {
	let out = '';
	for (let i = 0; i < 5; i++)
		out += at(0, 0, 1, i * 72, ellipse(0, -r * 0.6, r * 0.42, r * 0.6, '#ffc4d8'));
	return out + circle(0, 0, r * 0.25, '#ff8fb1');
}

/** A sprout, about 120 high, from its root at (0, 0). */
export function sprout() {
	return (
		line('M0 0 Q-6 -40 0 -80', C.stem, 12) +
		ellipse(-30, -86, 32, 18, C.leaf) +
		ellipse(30, -92, 32, 18, C.leaf)
	);
}

/** A winding road from the bottom up to a checkered flag. */
export function road() {
	return (
		line('M-160 200 C-40 160 120 120 40 40 C-40 -40 -120 -60 40 -120', '#3e4650', 70) +
		line('M-160 200 C-40 160 120 120 40 40 C-40 -40 -120 -60 40 -120', '#f5f1e4', 6, {
			dash: '20 24'
		}) +
		at(70, -116, 1, 0, flag(170, { checkered: true, id: 'road' }))
	);
}

/** A row of little screens, for mission control. */
export function screens() {
	let out = rect(-230, 40, 460, 80, 14, '#3e4650');
	for (const [x, y, c] of [
		[-150, -40, '#35b7c9'],
		[0, -60, '#8af0b4'],
		[150, -40, '#35b7c9']
	]) {
		out +=
			rect(x - 70, y - 50, 140, 100, 14, '#23342b') +
			rect(x - 60, y - 40, 120, 80, 10, c, { opacity: 0.85 });
	}
	out += line('M-200 -40 L-170 -20 L-140 -50 L-110 -30', '#ffffff', 5, { opacity: 0.7 });
	out += at(
		0,
		-60,
		0.18,
		0,
		poly(
			[
				[-100, -100],
				[100, 0],
				[-100, 100]
			],
			'#ffffff'
		)
	);
	return out;
}

/** Two masks of numbers that do not matter: a ballot-style score card. */
export function scoreCard(text, color = C.paper) {
	return rect(-90, -110, 180, 220, 18, color, { stroke: '#cfc8b6', sw: 4 }) + text;
}

export const misc = { circle, ellipse, rect, path, line, poly };
