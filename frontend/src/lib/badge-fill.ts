/**
 * Taking the background out of a badge picture, on plain pixels: no canvas,
 * no DOM, so it runs in a worker and in a unit test alike.
 *
 * It is a flood fill, not a guess at what the subject is. It starts from the
 * picture's border (and from any spot the person tapped) and clears only what
 * it can reach through colours close to where it started, so white inside a
 * letter stays white. Past the tolerance, a band of softness fades from clear
 * to solid, and the pixels in that band have the background taken out of
 * their colour, so a cut-out logo keeps no halo.
 */

export type Rgb = [number, number, number];

export interface FillOptions {
	/** 0–100: how far a colour may be from the background and still go. */
	tolerance: number;
	/** 0–100: the band past the tolerance that fades rather than cuts. */
	softness: number;
	/** The background, when the person picked it; else the border's own. */
	seed?: Rgb | null;
	/** Spots the border cannot reach, as pixel indexes, each cleared from its own colour. */
	spots?: number[];
}

/**
 * How deep the soft band reaches into the picture, in pixels. An edge drawn
 * smooth is two or three pixels wide at the sizes a badge is made at.
 */
const EDGE_DEPTH = 3;

/** The largest distance between two colours, black to white. */
const FAR = Math.sqrt(3 * 255 * 255);

const distance = (data: Uint8ClampedArray, i: number, c: Rgb) => {
	const r = data[i] - c[0];
	const g = data[i + 1] - c[1];
	const b = data[i + 2] - c[2];
	return (Math.sqrt(r * r + g * g + b * b) / FAR) * 100;
};

/**
 * The border's most common colour: the pixels round the edge, in coarse
 * buckets so a gradient's neighbours count together, averaged within the
 * winning bucket. Clear pixels do not vote.
 */
export function borderColour(data: Uint8ClampedArray, width: number, height: number): Rgb {
	const buckets = new Map<number, { n: number; r: number; g: number; b: number }>();
	const vote = (x: number, y: number) => {
		const i = (y * width + x) * 4;
		if (data[i + 3] < 128) return;
		const key = ((data[i] >> 4) << 8) | ((data[i + 1] >> 4) << 4) | (data[i + 2] >> 4);
		const b = buckets.get(key) ?? { n: 0, r: 0, g: 0, b: 0 };
		b.n++;
		b.r += data[i];
		b.g += data[i + 1];
		b.b += data[i + 2];
		buckets.set(key, b);
	};
	for (let x = 0; x < width; x++) {
		vote(x, 0);
		vote(x, height - 1);
	}
	for (let y = 1; y < height - 1; y++) {
		vote(0, y);
		vote(width - 1, y);
	}
	let best: { n: number; r: number; g: number; b: number } | null = null;
	for (const b of buckets.values()) if (!best || b.n > best.n) best = b;
	if (!best) return [255, 255, 255];
	return [Math.round(best.r / best.n), Math.round(best.g / best.n), Math.round(best.b / best.n)];
}

/** How solid a pixel at this distance from the background stays: 0 clear, 1 kept. */
export function coverage(d: number, tolerance: number, softness: number): number {
	if (d <= tolerance) return 0;
	if (softness <= 0 || d >= tolerance + softness) return 1;
	return (d - tolerance) / softness;
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
	const seed = options.seed ?? borderColour(data, width, height);
	const reach = options.tolerance + Math.max(0, options.softness);
	const total = width * height;
	/** How solid each pixel stays, 1 for anything the fill never reached. */
	const keep = new Float32Array(total).fill(1);
	/** The background each reached pixel is measured against. */
	const from = new Int32Array(total).fill(-1);
	const seen = new Uint8Array(total);
	const queue = new Int32Array(total);

	/** How many soft pixels in from the last clear one: the band is an edge, not a region. */
	const depth = new Uint8Array(total);

	const fill = (starts: number[], colour: Rgb, mark: number) => {
		let head = 0;
		let tail = 0;
		const visit = (p: number, parent: number) => {
			if (seen[p]) return;
			const i = p * 4;
			if (data[i + 3] === 0) {
				// Already clear: nothing to take out, but the fill goes on through.
				seen[p] = 1;
				keep[p] = 0;
				queue[tail++] = p;
				return;
			}
			const d = distance(data, i, colour);
			if (d >= reach) return;
			const solid = coverage(d, options.tolerance, options.softness);
			if (solid > 0) {
				// A soft pixel only counts beside the background: past a few of
				// them it is the picture's own pale colour, not an edge.
				const steps = parent < 0 ? 1 : depth[parent] + 1;
				if (steps > EDGE_DEPTH) return;
				depth[p] = steps;
			}
			seen[p] = 1;
			keep[p] = Math.min(keep[p], solid);
			from[p] = mark;
			queue[tail++] = p;
		};
		for (const p of starts) visit(p, -1);
		while (head < tail) {
			const p = queue[head++];
			const x = p % width;
			if (x > 0) visit(p - 1, p);
			if (x < width - 1) visit(p + 1, p);
			if (p >= width) visit(p - width, p);
			if (p < total - width) visit(p + width, p);
		}
	};

	const border: number[] = [];
	for (let x = 0; x < width; x++) border.push(x, (height - 1) * width + x);
	for (let y = 1; y < height - 1; y++) border.push(y * width, y * width + width - 1);
	const colours: Rgb[] = [seed];
	fill(border, seed, 0);

	for (const spot of options.spots ?? []) {
		if (spot < 0 || spot >= total || seen[spot]) continue;
		const i = spot * 4;
		const colour: Rgb = [data[i], data[i + 1], data[i + 2]];
		colours.push(colour);
		fill([spot], colour, colours.length - 1);
	}

	for (let p = 0; p < total; p++) {
		const a = keep[p];
		if (a >= 1) continue;
		const i = p * 4;
		if (a <= 0) {
			data[i + 3] = 0;
			continue;
		}
		const bg = colours[from[p]] ?? seed;
		data[i] = decontaminate(data[i], bg[0], a);
		data[i + 1] = decontaminate(data[i + 1], bg[1], a);
		data[i + 2] = decontaminate(data[i + 2], bg[2], a);
		data[i + 3] = Math.round(data[i + 3] * a);
	}
	return seed;
}
