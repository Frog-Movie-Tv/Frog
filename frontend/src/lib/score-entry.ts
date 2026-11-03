import type { ScoreFormat } from './format';

/**
 * Scores slid to rather than picked. A hundred points, or ten with a decimal,
 * are too many choices to lay out as buttons, so those formats take a slider
 * (`ScoreSlider`); this is its scale, and what keeps a value on it.
 */

/** The formats whose score is slid to. */
export function sliding(format: ScoreFormat): boolean {
	return format === 'pt100' || format === 'pt10dec';
}

/** The top of a slider's scale and the size of one step; the bottom is 0. */
export function slideScale(format: ScoreFormat): { max: number; step: number } {
	return format === 'pt100' ? { max: 100, step: 1 } : { max: 10, step: 0.1 };
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
