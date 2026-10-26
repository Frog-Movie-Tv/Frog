/**
 * The year in review card: a 1080x1920 PNG drawn on a canvas in the browser.
 *
 * Posters come through our own `/img/` proxy, so the canvas stays same-origin
 * and can be exported. Colours are read from the active theme, so the card
 * matches whatever the app looks like right now.
 */
import type { FullStats, StatsTitle } from './types';

export const REVIEW_WIDTH = 1080;
export const REVIEW_HEIGHT = 1920;

const PAD = 84;
const DISPLAY = '"Outfit Variable", "Inter Variable", system-ui, sans-serif';
const BODY = '"Inter Variable", system-ui, sans-serif';

interface Palette {
	bg: string;
	elev: string;
	text: string;
	dim: string;
	faint: string;
	accent: string;
	line: string;
}

function themePalette(): Palette {
	const css = getComputedStyle(document.documentElement);
	const v = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback;
	return {
		bg: v('--bg', '#07120e'),
		elev: v('--bg-elev', '#0e1f19'),
		text: v('--text', '#f2efe3'),
		dim: v('--text-dim', '#a9bdb0'),
		faint: v('--text-faint', '#6f8a7c'),
		accent: v('--accent', '#6cc46b'),
		line: v('--line-strong', 'rgba(242, 239, 227, 0.19)')
	};
}

function loadImage(src: string): Promise<HTMLImageElement | null> {
	return new Promise((resolve) => {
		const image = new Image();
		image.decoding = 'async';
		image.onload = () => resolve(image);
		image.onerror = () => resolve(null);
		image.src = src;
	});
}

/** Always the sharp size: the card is shared, so data saver does not apply. */
function poster(path: string | null): Promise<HTMLImageElement | null> {
	if (!path) return Promise.resolve(null);
	return loadImage(`/img/w342${path.startsWith('/') ? path : `/${path}`}`);
}

/** Fit text to a width over a few lines, ending in an ellipsis if it runs over. */
function wrap(ctx: CanvasRenderingContext2D, text: string, width: number, lines: number): string[] {
	const words = text.split(/\s+/);
	const out: string[] = [];
	let line = '';
	for (const word of words) {
		const next = line ? `${line} ${word}` : word;
		if (ctx.measureText(next).width <= width || !line) {
			line = next;
			continue;
		}
		out.push(line);
		line = word;
		if (out.length === lines) break;
	}
	if (out.length < lines && line) out.push(line);
	if (out.length === lines && out.join(' ').length < text.length) {
		let last = out[lines - 1];
		while (last.length > 1 && ctx.measureText(`${last}…`).width > width) last = last.slice(0, -1);
		out[lines - 1] = `${last.trimEnd()}…`;
	}
	return out;
}

function drawPoster(
	ctx: CanvasRenderingContext2D,
	image: HTMLImageElement | null,
	title: StatsTitle,
	x: number,
	y: number,
	w: number,
	h: number,
	p: Palette
) {
	ctx.save();
	ctx.beginPath();
	ctx.roundRect(x, y, w, h, 18);
	ctx.clip();
	if (image) {
		ctx.drawImage(image, x, y, w, h);
	} else {
		ctx.fillStyle = p.elev;
		ctx.fillRect(x, y, w, h);
		ctx.fillStyle = p.dim;
		ctx.font = `600 26px ${BODY}`;
		ctx.textAlign = 'center';
		wrap(ctx, title.name, w - 28, 4).forEach((l, i) => ctx.fillText(l, x + w / 2, y + 60 + i * 34));
		ctx.textAlign = 'left';
	}
	ctx.restore();
	ctx.strokeStyle = p.line;
	ctx.lineWidth = 2;
	ctx.beginPath();
	ctx.roundRect(x, y, w, h, 18);
	ctx.stroke();
}

function eyebrow(
	ctx: CanvasRenderingContext2D,
	text: string,
	x: number,
	y: number,
	p: Palette,
	size = 26
) {
	ctx.fillStyle = p.faint;
	ctx.font = `700 ${size}px ${BODY}`;
	ctx.letterSpacing = `${Math.round(size / 8)}px`;
	ctx.fillText(text.toUpperCase(), x, y);
	ctx.letterSpacing = '0px';
}

const nf = new Intl.NumberFormat();

/** Draw the card. Resolves once every poster has loaded or failed. */
export async function renderReview(stats: FullStats, year: number): Promise<HTMLCanvasElement> {
	const p = themePalette();
	await Promise.all([
		document.fonts.load(`700 200px ${DISPLAY}`),
		document.fonts.load(`600 40px ${BODY}`),
		document.fonts.load(`700 26px ${BODY}`)
	]).catch(() => undefined);

	const show = stats.top_shows[0] ?? null;
	const films = stats.top_films.slice(0, 4);
	const [showImage, ...filmImages] = await Promise.all([
		poster(show?.poster_path ?? null),
		...films.map((f) => poster(f.poster_path))
	]);

	const canvas = document.createElement('canvas');
	canvas.width = REVIEW_WIDTH;
	canvas.height = REVIEW_HEIGHT;
	const ctx = canvas.getContext('2d');
	if (!ctx) throw new Error('This browser cannot draw the card.');

	// Background: the theme, with the accent glowing in from the top corner.
	ctx.fillStyle = p.bg;
	ctx.fillRect(0, 0, REVIEW_WIDTH, REVIEW_HEIGHT);
	ctx.save();
	ctx.globalAlpha = 0.28;
	const glow = ctx.createRadialGradient(REVIEW_WIDTH, 0, 0, REVIEW_WIDTH, 0, 1100);
	glow.addColorStop(0, p.accent);
	glow.addColorStop(1, 'transparent');
	ctx.fillStyle = glow;
	ctx.fillRect(0, 0, REVIEW_WIDTH, REVIEW_HEIGHT);
	ctx.restore();

	ctx.textBaseline = 'alphabetic';

	// Header
	ctx.fillStyle = p.accent;
	ctx.font = `700 34px ${DISPLAY}`;
	ctx.fillText('Frog', PAD, 140);
	ctx.textAlign = 'right';
	eyebrow(ctx, 'Year in review', REVIEW_WIDTH - PAD, 138, p);
	ctx.textAlign = 'left';

	// The year
	ctx.fillStyle = p.text;
	ctx.font = `700 230px ${DISPLAY}`;
	ctx.letterSpacing = '-8px';
	ctx.fillText(String(year), PAD - 8, 390);
	ctx.letterSpacing = '0px';

	// Time watched, as the headline
	const hours = Math.round(stats.totals.minutes / 60);
	ctx.fillStyle = p.accent;
	ctx.font = `700 160px ${DISPLAY}`;
	ctx.fillText(nf.format(hours), PAD - 4, 590);
	ctx.fillStyle = p.dim;
	ctx.font = `600 40px ${BODY}`;
	ctx.fillText(hours === 1 ? 'hour of film and TV' : 'hours of film and TV', PAD, 652);

	// Three counts
	const counts: [number, string][] = [
		[stats.totals.films, stats.totals.films === 1 ? 'film' : 'films'],
		[stats.totals.episodes, stats.totals.episodes === 1 ? 'episode' : 'episodes'],
		[stats.totals.days, stats.totals.days === 1 ? 'day watching' : 'days watching']
	];
	const col = (REVIEW_WIDTH - PAD * 2) / 3;
	counts.forEach(([n, label], i) => {
		const x = PAD + i * col;
		ctx.fillStyle = p.text;
		ctx.font = `700 84px ${DISPLAY}`;
		ctx.fillText(nf.format(n), x, 776);
		ctx.fillStyle = p.faint;
		ctx.font = `600 30px ${BODY}`;
		ctx.fillText(label, x, 822);
	});

	// Most watched show
	let y = 890;
	if (show) {
		drawPoster(ctx, showImage, show, PAD, y, 200, 300, p);
		const tx = PAD + 200 + 44;
		const tw = REVIEW_WIDTH - tx - PAD;
		eyebrow(ctx, 'Most watched show', tx, y + 46, p);
		ctx.fillStyle = p.text;
		ctx.font = `700 60px ${DISPLAY}`;
		const lines = wrap(ctx, show.name, tw, 2);
		lines.forEach((l, i) => ctx.fillText(l, tx, y + 124 + i * 68));
		ctx.fillStyle = p.dim;
		ctx.font = `600 34px ${BODY}`;
		ctx.fillText(
			`${nf.format(show.watches)} ${show.watches === 1 ? 'episode' : 'episodes'}`,
			tx,
			y + 124 + lines.length * 68 + 10
		);
		y += 300 + 56;
	}

	// Top films
	if (films.length) {
		eyebrow(ctx, films.length === 1 ? 'Top film' : 'Top films', PAD, y + 26, p);
		const gap = 28;
		const w = (REVIEW_WIDTH - PAD * 2 - gap * 3) / 4;
		const h = w * 1.5;
		films.forEach((film, i) =>
			drawPoster(ctx, filmImages[i], film, PAD + i * (w + gap), y + 56, w, h, p)
		);
		y += 56 + h + 64;
	}

	// Three facts along the bottom, each allowed two lines for long names.
	const facts: [string, string][] = [];
	if (stats.genres[0]) facts.push(['Top genre', stats.genres[0].name]);
	if (stats.streaks.longest.days > 1)
		facts.push(['Longest streak', `${stats.streaks.longest.days} days`]);
	if (stats.people.actors[0] && stats.people.actors[0].titles > 1)
		facts.push(['Most seen', stats.people.actors[0].name]);
	const factY = Math.min(y, REVIEW_HEIGHT - 190);
	facts.slice(0, 3).forEach(([label, value], i) => {
		const x = PAD + i * col;
		eyebrow(ctx, label, x, factY, p, 22);
		ctx.fillStyle = p.text;
		ctx.font = `700 38px ${DISPLAY}`;
		wrap(ctx, value, col - 28, 2).forEach((line, j) => ctx.fillText(line, x, factY + 52 + j * 44));
	});

	return canvas;
}

export function toBlob(canvas: HTMLCanvasElement): Promise<Blob> {
	return new Promise((resolve, reject) =>
		canvas.toBlob(
			(b) => (b ? resolve(b) : reject(new Error('Could not create the image.'))),
			'image/png'
		)
	);
}
