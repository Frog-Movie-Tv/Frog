/**
 * Making a badge, in the browser: read the upload, crop it to a circle, take
 * out the background, put a fill and a ring behind it, and finish it as two
 * lossless PNGs. The server only checks and stores the result.
 *
 * Unlike `picture.ts`, nothing here ever falls back to JPEG: a badge keeps
 * its transparency. An SVG is drawn to pixels at full size and only those
 * pixels are kept; the SVG itself is never stored or served.
 */
import type { FillOptions, Rgb } from './badge-fill';
import { isAnimated } from './picture';
import { PALETTE, type ProfileColour, type ProfileShade } from './profile-colours';

/** An SVG is drawn this large on its long side, and so is anything shrunk. */
export const SOURCE_PIXELS = 2048;
/** An upload past this is shrunk to `SOURCE_PIXELS` before it is kept. */
export const ORIGINAL_MAX_BYTES = 4 * 1024 * 1024;
/** What the server takes as the original, at most. */
const ORIGINAL_LIMIT = 8 * 1024 * 1024;
/** Opened large, and drawn everywhere else. */
export const LARGE = 1024;
export const SMALL = 128;
/** Cropping less than this from the picture makes a soft large badge. */
export const SHARP_PIXELS = 512;
/** The preview's working size. */
export const PREVIEW = 512;

export type Fill = 'none' | 'white' | ProfileColour;

/** Everything the editor was left at. Kept with the badge, so editing resumes there. */
export interface BadgeSettings {
	/** The square cut from the picture, in its pixels: may reach past its edges. */
	crop: { x: number; y: number; size: number };
	removal: {
		on: boolean;
		tolerance: number;
		softness: number;
		/** The background, when picked by tapping; otherwise read from the border. */
		seed: Rgb | null;
		/** Spots tapped to clear, as fractions of the picture's width and height. */
		spots: { x: number; y: number }[];
	};
	fill: Fill;
	shade: ProfileShade;
	ring: boolean;
}

export function startingSettings(width: number, height: number): BadgeSettings {
	const size = Math.min(width, height);
	return {
		crop: { x: (width - size) / 2, y: (height - size) / 2, size },
		removal: { on: false, tolerance: 10, softness: 40, seed: null, spots: [] },
		fill: 'none',
		shade: 'medium',
		ring: false
	};
}

/** Settings read back from a saved badge, with anything missing filled in. */
export function restoreSettings(
	saved: Record<string, unknown>,
	width: number,
	height: number
): BadgeSettings {
	const start = startingSettings(width, height);
	const s = saved as Partial<BadgeSettings>;
	const crop = s.crop;
	const number = (v: unknown, d: number) => (typeof v === 'number' && Number.isFinite(v) ? v : d);
	return {
		crop:
			crop && number(crop.size, 0) > 0
				? { x: number(crop.x, 0), y: number(crop.y, 0), size: number(crop.size, 1) }
				: start.crop,
		removal: { ...start.removal, ...(s.removal ?? {}) },
		fill: s.fill ?? start.fill,
		shade: s.shade ?? start.shade,
		ring: s.ring ?? start.ring
	};
}

/** The picture being made into a badge: pixels to draw from, and what to keep. */
export interface Source {
	canvas: HTMLCanvasElement;
	width: number;
	height: number;
	/** What is stored as the original: always pixels. */
	original: Blob;
	/** For the frame the person drags in. */
	url: string;
}

const UNREADABLE = 'That picture could not be read. Try a PNG, JPEG, WebP or SVG.';

const canvasOf = (width: number, height: number) => {
	const c = document.createElement('canvas');
	c.width = Math.max(1, Math.round(width));
	c.height = Math.max(1, Math.round(height));
	return c;
};

const context = (c: HTMLCanvasElement) => {
	const ctx = c.getContext('2d', { willReadFrequently: false });
	if (!ctx) throw new Error(UNREADABLE);
	ctx.imageSmoothingEnabled = true;
	ctx.imageSmoothingQuality = 'high';
	return ctx;
};

export const toPng = (c: HTMLCanvasElement) =>
	new Promise<Blob>((resolve, reject) =>
		c.toBlob((b) => (b ? resolve(b) : reject(new Error(UNREADABLE))), 'image/png')
	);

const toJpeg = (c: HTMLCanvasElement) =>
	new Promise<Blob>((resolve, reject) =>
		c.toBlob((b) => (b ? resolve(b) : reject(new Error(UNREADABLE))), 'image/jpeg', 0.92)
	);

async function isSvg(file: Blob): Promise<boolean> {
	if (file.type === 'image/svg+xml') return true;
	const head = (await file.slice(0, 512).text()).trimStart().toLowerCase();
	return head.startsWith('<svg') || (head.startsWith('<?xml') && head.includes('<svg'));
}

/** An SVG's size: its own, or its viewBox's shape, scaled to the long side. */
async function drawSvg(file: Blob): Promise<HTMLCanvasElement> {
	const text = await file.text();
	const url = URL.createObjectURL(new Blob([text], { type: 'image/svg+xml' }));
	try {
		const image = new Image();
		image.src = url;
		await image.decode().catch(() => {
			throw new Error(UNREADABLE);
		});
		let w = image.naturalWidth;
		let h = image.naturalHeight;
		const box = text.match(
			/viewBox\s*=\s*["']\s*[-\d.]+[\s,]+[-\d.]+[\s,]+([\d.]+)[\s,]+([\d.]+)/i
		);
		if (box && (!w || !h || text.match(/<svg[^>]*\swidth\s*=\s*["'][\d.]+%/i))) {
			w = Number(box[1]);
			h = Number(box[2]);
		}
		if (!w || !h) w = h = SOURCE_PIXELS;
		const scale = SOURCE_PIXELS / Math.max(w, h);
		const canvas = canvasOf(w * scale, h * scale);
		context(canvas).drawImage(image, 0, 0, canvas.width, canvas.height);
		return canvas;
	} finally {
		URL.revokeObjectURL(url);
	}
}

/** The first frame of anything else, at most `longest` on its long side. */
async function drawRaster(file: Blob, longest = Infinity): Promise<HTMLCanvasElement> {
	let bitmap: ImageBitmap;
	try {
		bitmap = await createImageBitmap(file);
	} catch {
		throw new Error(UNREADABLE);
	}
	const scale = Math.min(1, longest / Math.max(bitmap.width, bitmap.height));
	const canvas = canvasOf(bitmap.width * scale, bitmap.height * scale);
	context(canvas).drawImage(bitmap, 0, 0, canvas.width, canvas.height);
	bitmap.close();
	return canvas;
}

/**
 * Read an upload, or a badge's stored original. What is kept is the file
 * itself when it is a still PNG, JPEG or WebP small enough; otherwise its
 * pixels, shrunk to `SOURCE_PIXELS` on the long side. PNG keeps any
 * transparency; only a JPEG upload stays a JPEG.
 */
export async function readSource(file: Blob): Promise<Source> {
	if (await isSvg(file)) {
		const canvas = await drawSvg(file);
		const original = await toPng(canvas);
		return {
			canvas,
			width: canvas.width,
			height: canvas.height,
			original,
			url: URL.createObjectURL(original)
		};
	}
	const type = file.type;
	const still =
		['image/png', 'image/jpeg', 'image/webp'].includes(type) &&
		!isAnimated(new Uint8Array(await file.arrayBuffer()));
	const full = await drawRaster(file);
	const long = Math.max(full.width, full.height);
	const keep =
		still &&
		(file.size <= ORIGINAL_MAX_BYTES || (long <= SOURCE_PIXELS && file.size <= ORIGINAL_LIMIT));
	if (keep) {
		return {
			canvas: full,
			width: full.width,
			height: full.height,
			original: file,
			url: URL.createObjectURL(file)
		};
	}
	let canvas = long > SOURCE_PIXELS ? await drawRaster(file, SOURCE_PIXELS) : full;
	let original = type === 'image/jpeg' ? await toJpeg(canvas) : await toPng(canvas);
	// A detailed picture can still run past what is kept; a little smaller does it.
	if (original.size > ORIGINAL_LIMIT) {
		canvas = await drawRaster(file, SOURCE_PIXELS * 0.75);
		original = await toPng(canvas);
	}
	return {
		canvas,
		width: canvas.width,
		height: canvas.height,
		original,
		url: URL.createObjectURL(original)
	};
}

/** The colour at one spot of the picture. */
export function colourAt(source: Source, x: number, y: number): Rgb {
	const probe = canvasOf(1, 1);
	const ctx = probe.getContext('2d', { willReadFrequently: true });
	if (!ctx) return [255, 255, 255];
	ctx.drawImage(source.canvas, Math.floor(x), Math.floor(y), 1, 1, 0, 0, 1, 1);
	const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data;
	return [r, g, b];
}

/** The fill behind the picture: the bright side of a profile colour. */
export function fillColour(fill: Fill, shade: ProfileShade): string | null {
	if (fill === 'none') return null;
	if (fill === 'white') return '#ffffff';
	return PALETTE[fill][shade].dark;
}

/** A thin ring round the edge, a step darker than the fill it frames. */
export function ringColour(fill: Fill, shade: ProfileShade): string {
	if (fill === 'none') return '#ffffff';
	if (fill === 'white') return '#d5d5de';
	return PALETTE[fill][shade].light;
}

/** Runs the background fill somewhere off the main thread, or here. */
export type Filler = (
	data: Uint8ClampedArray<ArrayBuffer>,
	width: number,
	height: number,
	options: FillOptions
) => Promise<{ data: Uint8ClampedArray<ArrayBuffer>; seed: Rgb }>;

/** A filler that sends each picture to the worker, one answer per job. */
export function workerFiller(worker: Worker): Filler {
	let next = 1;
	const waiting = new Map<
		number,
		(a: { data: Uint8ClampedArray<ArrayBuffer>; seed: Rgb }) => void
	>();
	worker.onmessage = (event) => {
		const { id, data, seed } = event.data;
		waiting.get(id)?.({ data, seed });
		waiting.delete(id);
	};
	return (data, width, height, options) =>
		new Promise((resolve) => {
			const id = next++;
			waiting.set(id, resolve);
			worker.postMessage({ id, data, width, height, options }, [data.buffer]);
		});
}

/**
 * The badge at `size` pixels: the crop drawn, the background taken out,
 * the fill behind it, cut to a circle with a smooth edge, and the ring.
 * Answers the background colour the fill cleared, for the editor to show.
 */
export async function render(
	source: Source,
	s: BadgeSettings,
	size: number,
	filler: Filler
): Promise<{ canvas: HTMLCanvasElement; seed: Rgb | null }> {
	const k = size / s.crop.size;
	const picture = canvasOf(size, size);
	const pctx = context(picture);
	// Drawn whole and moved, so a crop reaching past the picture's edges
	// leaves clear space rather than smearing its last row.
	pctx.drawImage(source.canvas, -s.crop.x * k, -s.crop.y * k, source.width * k, source.height * k);

	let seed: Rgb | null = null;
	if (s.removal.on) {
		const image = pctx.getImageData(0, 0, size, size);
		const spots = s.removal.spots
			.map(({ x, y }) => {
				const px = Math.floor((x * source.width - s.crop.x) * k);
				const py = Math.floor((y * source.height - s.crop.y) * k);
				return px >= 0 && py >= 0 && px < size && py < size ? py * size + px : -1;
			})
			.filter((p) => p >= 0);
		const done = await filler(image.data as Uint8ClampedArray<ArrayBuffer>, size, size, {
			tolerance: s.removal.tolerance,
			softness: s.removal.softness,
			seed: s.removal.seed,
			spots
		});
		seed = done.seed;
		pctx.putImageData(new ImageData(done.data, size, size), 0, 0);
	}

	const out = canvasOf(size, size);
	const ctx = context(out);
	const r = size / 2;
	const fill = fillColour(s.fill, s.shade);
	if (fill) {
		ctx.fillStyle = fill;
		ctx.fillRect(0, 0, size, size);
	}
	ctx.drawImage(picture, 0, 0);
	// The circle, its edge smoothed by the fill that draws it.
	ctx.globalCompositeOperation = 'destination-in';
	ctx.beginPath();
	ctx.arc(r, r, r, 0, Math.PI * 2);
	ctx.fill();
	ctx.globalCompositeOperation = 'source-over';
	if (s.ring) {
		const width = Math.max(1, size * 0.035);
		ctx.strokeStyle = ringColour(s.fill, s.shade);
		ctx.lineWidth = width;
		ctx.beginPath();
		ctx.arc(r, r, r - width / 2, 0, Math.PI * 2);
		ctx.stroke();
	}
	return { canvas: out, seed };
}

/**
 * Smaller by halves, each step smoothed, down to `size`: one big step drops
 * most pixels and leaves a 20 px badge jagged, many small ones keep it crisp.
 */
export function halve(canvas: HTMLCanvasElement, size: number): HTMLCanvasElement {
	let current = canvas;
	while (current.width / 2 >= size) {
		const next = canvasOf(current.width / 2, current.height / 2);
		context(next).drawImage(current, 0, 0, next.width, next.height);
		current = next;
	}
	if (current.width !== size) {
		const last = canvasOf(size, size);
		context(last).drawImage(current, 0, 0, size, size);
		current = last;
	}
	return current;
}

/** Bytes as base64, the way the badge routes take them. */
export function base64(blob: Blob): Promise<string> {
	return new Promise((resolve, reject) => {
		const reader = new FileReader();
		reader.onload = () => resolve(String(reader.result).split(',', 2)[1] ?? '');
		reader.onerror = () => reject(new Error(UNREADABLE));
		reader.readAsDataURL(blob);
	});
}

/** Keep the crop's centre on the picture, and its size within reason. */
export function clampCrop(
	crop: BadgeSettings['crop'],
	width: number,
	height: number
): BadgeSettings['crop'] {
	const size = Math.min(Math.max(crop.size, 24), Math.max(width, height) * 1.6);
	const cx = Math.min(Math.max(crop.x + crop.size / 2, 0), width);
	const cy = Math.min(Math.max(crop.y + crop.size / 2, 0), height);
	return { x: cx - size / 2, y: cy - size / 2, size };
}
