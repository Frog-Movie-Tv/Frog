import type { ScoreFormat } from './format';

/**
 * How a score is entered. The numbers, a hundred points or ten with or
 * without a decimal, are slid to (`ScoreSlider`); stars are filled in
 * (`StarPicker`); smileys are three buttons. The scales and what keeps a value
 * on them are here, so they are tested without a page.
 */

/** The formats whose score is slid to. */
export function sliding(format: ScoreFormat): boolean {
	return format === 'pt100' || format === 'pt10dec' || format === 'pt10';
}

/** The top of a slider's scale and the size of one step; the bottom is 0. */
export function slideScale(format: ScoreFormat): { max: number; step: number } {
	if (format === 'pt100') return { max: 100, step: 1 };
	if (format === 'pt10') return { max: 10, step: 1 };
	return { max: 10, step: 0.1 };
}

/**
 * A value held to the scale: no lower than 0, no higher than the top, and on
 * a step, so a hundred points is a whole number and ten has one decimal.
 */
export function snap(display: number, format: ScoreFormat): number {
	const { max, step } = slideScale(format);
	if (!Number.isFinite(display)) return 0;
	const held = Math.min(max, Math.max(0, display));
	return Math.round(Math.round(held / step) * step * 10) / 10;
}

/** The numbers under a slider's track. */
export function marks(format: ScoreFormat): number[] {
	return format === 'pt100' ? [0, 25, 50, 75, 100] : [0, 2, 4, 6, 8, 10];
}

/** How many stars a row of them holds. */
export const STARS = 5;

/**
 * The stars a touch fills, from how far along the row it lands (0 at the left
 * edge, 1 at the right). The left half of a star is half of it, when halves
 * are on; otherwise any part of a star is the whole star. Never less than the
 * smallest score: a touch on the row always gives one.
 */
export function starsAt(along: number, halves: boolean): number {
	const unit = halves ? 0.5 : 1;
	const held = Math.min(1, Math.max(0, Number.isFinite(along) ? along : 0));
	const filled = Math.ceil((held * STARS) / unit - 1e-9) * unit;
	return Math.min(STARS, Math.max(unit, filled));
}

/** One step more or fewer stars, as an arrow key moves them. */
export function starStep(stars: number, by: number, halves: boolean): number {
	const unit = halves ? 0.5 : 1;
	const on = Math.round(stars / unit) * unit;
	return Math.min(STARS, Math.max(unit, on + by * unit));
}
