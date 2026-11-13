/**
 * Taking the background out of a badge picture, on plain pixels: no canvas,
 * no DOM, so it runs in a worker and in a unit test alike.
 *
 * It is a flood fill, not a guess at what the subject is. It starts from the
 * picture's border (and from any spot the person tapped) and clears only what
 * it can reach through colours close to where it started, so white inside a
 * letter stays white. Colours are compared in Oklab, where a step means the
 * same to the eye whatever the colour, so a pale pink is not taken for white.
 *
 * The edge is where a cut-out usually goes wrong: a smooth edge's pixels are
 * part picture, part background, and kept as they are they leave a pale
 * fringe. Each pixel near the cleared part is unmixed instead: how much of it
 * is the picture is read from where it falls between the background and the
 * picture's colour just inside it, and the background is taken out of what
 * is left. Trim takes a pixel or more off the outside first, for pictures
 * whose edge is blurred wider than that.
 */

export type Rgb = [number, number, number];

export interface FillOptions {
	/** 0–40: how far a colour may be from the background and still go, in Oklab hundredths. */
	tolerance: number;
	/** 0–3: pixels taken off the outside of what stays, before the edge is smoothed. */
	trim?: number;
	/** The background, when the person picked it; else the border's own. */
	seed?: Rgb | null;
	/** Spots the border cannot reach, as pixel indexes, each cleared from its own colour. */
	spots?: number[];
}

/** How deep the smoothed edge reaches into what stays, in pixels. */
const EDGE = 3;
/** How far round an edge pixel to look for the picture's own colour. */
const LOOK = 4;

const linear = (c: number) => {
	const v = c / 255;
	return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
};

/** sRGB to Oklab, each part times 100, so a tolerance reads in whole numbers. */
export function oklab(r: number, g: number, b: number): Rgb {
	const lr = linear(r);
	const lg = linear(g);
	const lb = linear(b);
	const l = Math.cbrt(0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb);
	const m = Math.cbrt(0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb);
	const s = Math.cbrt(0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb);
	return [
		(0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s) * 100,
		(1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s) * 100,
		(0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s) * 100
	];
}

/** How far apart two colours look, from 0 (the same) to 100 (black and white). */
export function difference(a: Rgb, b: Rgb): number {
	const x = oklab(...a);
	const y = oklab(...b);
	return Math.hypot(x[0] - y[0], x[1] - y[1], x[2] - y[2]);
}

function border(width: number, height: number): number[] {
	const out: number[] = [];
	for (let x = 0; x < width; x++) out.push(x, (height - 1) * width + x);
	for (let y = 1; y < height - 1; y++) out.push(y * width, y * width + width - 1);
	return out;
}

/**
 * The border's most common colour: the pixels round the edge, in coarse
 * buckets so a gradient's neighbours count together, averaged within the
 * winning bucket. Clear pixels do not vote.
 */
export function borderColour(data: Uint8ClampedArray, width: number, height: number): Rgb {
	const buckets = new Map<number, { n: number; r: number; g: number; b: number }>();
	for (const p of border(width, height)) {
		const i = p * 4;
		if (data[i + 3] < 128) continue;
		const key = ((data[i] >> 4) << 8) | ((data[i + 1] >> 4) << 4) | (data[i + 2] >> 4);
		const b = buckets.get(key) ?? { n: 0, r: 0, g: 0, b: 0 };
		b.n++;
		b.r += data[i];
		b.g += data[i + 1];
		b.b += data[i + 2];
		buckets.set(key, b);
	}
	let best: { n: number; r: number; g: number; b: number } | null = null;
	for (const b of buckets.values()) if (!best || b.n > best.n) best = b;
	if (!best) return [255, 255, 255];
	return [Math.round(best.r / best.n), Math.round(best.g / best.n), Math.round(best.b / best.n)];
}

/**
 * A strength to start from: enough to take the background's own grain and
 * shading (a JPEG's noise, a soft gradient), measured on the border, and not
 * much more. Most pictures need nothing else.
 */
export function suggestTolerance(data: Uint8ClampedArray, width: number, height: number): number {
	const seed = oklab(...borderColour(data, width, height));
	const near: number[] = [];
	for (const p of border(width, height)) {
		const i = p * 4;
		if (data[i + 3] < 128) continue;
		const c = oklab(data[i], data[i + 1], data[i + 2]);
		const d = Math.hypot(c[0] - seed[0], c[1] - seed[1], c[2] - seed[2]);
		// Only what is plainly background: the picture touching the border does not count.
		if (d < 20) near.push(d);
	}
	if (!near.length) return 8;
	near.sort((a, b) => a - b);
	const spread = near[Math.floor((near.length - 1) * 0.95)];
	return Math.round(Math.min(30, Math.max(5, spread * 1.6 + 4)));
}

/**
 * How much of a pixel is the picture, from where its colour falls between the
 * background's and the picture's: observed = a·picture + (1 − a)·background.
 * Nothing when the two are too alike to tell apart.
 */
export function unmix(observed: Rgb, background: Rgb, picture: Rgb): number | null {
	let num = 0;
	let den = 0;
	for (let c = 0; c < 3; c++) {
		const f = picture[c] - background[c];
		num += (observed[c] - background[c]) * f;
		den += f * f;
	}
	if (den < 24 * 24) return null;
	return Math.min(1, Math.max(0, num / den));
}

/**
 * A colour seen over a background at some coverage, with the background
 * taken out: observed = a·c + (1 − a)·bg, so c = (observed − (1 − a)·bg) / a.
 */
export function decontaminate(observed: number, background: number, a: number): number {
	if (a <= 0) return observed;
	const c = (observed - (1 - a) * background) / a;
	return Math.max(0, Math.min(255, Math.round(c)));
}

/**
 * Clear the background of an RGBA picture in place. Answers the colour
 * cleared from the border, for the editor to show.
 */
export function removeBackground(
	data: Uint8ClampedArray,
	width: number,
	height: number,
	options: FillOptions
): Rgb {
	const total = width * height;
	const seed = options.seed ?? borderColour(data, width, height);
	const lab = new Float32Array(total * 3);
	for (let p = 0; p < total; p++) {
		const [l, a, b] = oklab(data[p * 4], data[p * 4 + 1], data[p * 4 + 2]);
		lab[p * 3] = l;
		lab[p * 3 + 1] = a;
		lab[p * 3 + 2] = b;
	}
	const far = (p: number, c: Rgb) =>
		Math.hypot(lab[p * 3] - c[0], lab[p * 3 + 1] - c[1], lab[p * 3 + 2] - c[2]);

	const neighbours = (p: number, visit: (q: number) => void) => {
		const x = p % width;
		if (x > 0) visit(p - 1);
		if (x < width - 1) visit(p + 1);
		if (p >= width) visit(p - width);
		if (p < total - width) visit(p + width);
	};

	/** Which background cleared each pixel, by its place in `colours`; -1 kept. */
	const cleared = new Int16Array(total).fill(-1);
	const colours: Rgb[] = [];
	const queue = new Int32Array(total);

	const fill = (starts: number[], colour: Rgb) => {
		const mark = colours.push(colour) - 1;
		const target = oklab(...colour);
		let head = 0;
		let tail = 0;
		const visit = (p: number) => {
			if (cleared[p] !== -1) return;
			// Already clear, or close enough to the background: it goes.
			if (data[p * 4 + 3] !== 0 && far(p, target) > options.tolerance) return;
			cleared[p] = mark;
			queue[tail++] = p;
		};
		for (const p of starts) visit(p);
		while (head < tail) neighbours(queue[head++], visit);
	};

	fill(border(width, height), seed);
	for (const spot of options.spots ?? []) {
		if (spot < 0 || spot >= total || cleared[spot] !== -1) continue;
		const i = spot * 4;
		fill([spot], [data[i], data[i + 1], data[i + 2]]);
	}

	// How far each kept pixel is from the cleared part, out to the trim and
	// the edge band (0 cleared, 1 touching it), and which background it is next to.
	const trim = Math.max(0, Math.min(3, Math.round(options.trim ?? 0)));
	const reach = EDGE + trim;
	const depth = new Uint8Array(total).fill(255);
	const nextTo = new Int16Array(total).fill(-1);
	let head = 0;
	let tail = 0;
	for (let p = 0; p < total; p++) {
		if (cleared[p] === -1) continue;
		depth[p] = 0;
		nextTo[p] = cleared[p];
		queue[tail++] = p;
	}
	const labOf = colours.map((c) => oklab(...c));
	/** Plainly picture: an edge stops here, and never runs on past it. */
	const solid = (q: number, mark: number) =>
		far(q, labOf[mark] ?? labOf[0]) >= options.tolerance + 25;
	while (head < tail) {
		const p = queue[head++];
		if (depth[p] >= reach) continue;
		// The edge walks in from the cleared part only through what could be
		// a mix of background and picture: a dark outline stops it, so the
		// white inside a letter is never taken for background.
		if (depth[p] > 0 && solid(p, nextTo[p])) continue;
		neighbours(p, (q) => {
			if (depth[q] !== 255) return;
			depth[q] = depth[p] + 1;
			nextTo[q] = nextTo[p];
			queue[tail++] = q;
		});
	}

	const out = data.slice();
	for (let p = 0; p < total; p++) {
		const i = p * 4;
		const d = depth[p];
		if (d === 0 || d <= trim) {
			out[i + 3] = 0;
			continue;
		}
		if (d === 255) continue;

		// The picture's own colour just inside: the average of the solid
		// pixels round this one, past the edge band.
		const x = p % width;
		const y = (p - x) / width;
		let n = 0;
		let r = 0;
		let g = 0;
		let b = 0;
		for (let dy = -LOOK; dy <= LOOK; dy++) {
			const yy = y + dy;
			if (yy < 0 || yy >= height) continue;
			for (let dx = -LOOK; dx <= LOOK; dx++) {
				const xx = x + dx;
				if (xx < 0 || xx >= width) continue;
				const q = yy * width + xx;
				// Solid picture only: not the edge, and nothing like the background.
				if (depth[q] !== 255 || data[q * 4 + 3] < 250) continue;
				if (!solid(q, nextTo[p])) continue;
				const k = q * 4;
				r += data[k];
				g += data[k + 1];
				b += data[k + 2];
				n++;
			}
		}
		const observed: Rgb = [data[i], data[i + 1], data[i + 2]];
		const bg = colours[nextTo[p]] ?? seed;
		let a: number | null = n ? unmix(observed, bg, [r / n, g / n, b / n]) : null;
		if (a === null) {
			// No picture colour to read from (a thin line, or one much like the
			// background): go by how far it is from the background.
			const gap = far(p, oklab(...bg));
			a = Math.min(1, Math.max(0, (gap - options.tolerance) / (options.tolerance + 12)));
		}
		if (a < 0.04) a = 0;
		else if (a > 0.97) a = 1;
		out[i] = decontaminate(observed[0], bg[0], a);
		out[i + 1] = decontaminate(observed[1], bg[1], a);
		out[i + 2] = decontaminate(observed[2], bg[2], a);
		out[i + 3] = Math.round(data[i + 3] * a);
	}
	data.set(out);
	return seed;
}
