/**
 * Ratings live as 0..100 in the database no matter how they are shown, so the
 * score format is purely a display concern and switching it never rewrites
 * data or loses precision.
 */
export type ScoreFormat = 'pt100' | 'pt10dec' | 'pt10' | 'star5' | 'smiley3';

export const SCORE_FORMATS: { id: ScoreFormat; label: string; example: string }[] = [
	{ id: 'pt100', label: '100 point', example: '87' },
	{ id: 'pt10dec', label: '10 point decimal', example: '8.7' },
	{ id: 'pt10', label: '10 point', example: '9' },
	{ id: 'star5', label: '5 stars', example: '★★★★½' },
	{ id: 'smiley3', label: '3 smileys', example: '🙂' }
];

export interface ScoreScale {
	max: number;
	step: number;
}

export function scale(format: ScoreFormat): ScoreScale {
	switch (format) {
		case 'pt100':
			return { max: 100, step: 1 };
		case 'pt10dec':
			return { max: 10, step: 0.1 };
		case 'pt10':
			return { max: 10, step: 1 };
		case 'star5':
			return { max: 5, step: 0.5 };
		case 'smiley3':
			return { max: 3, step: 1 };
	}
}

/** 0..100 to the number the chosen widget shows. */
export function toDisplay(value: number | null, format: ScoreFormat): number | null {
	if (value === null || value === undefined) return null;
	const { max, step } = scale(format);
	const raw = (value / 100) * max;
	return Math.round(raw / step) * step;
}

/** The widget's number back to 0..100. */
export function fromDisplay(value: number | null, format: ScoreFormat): number | null {
	if (value === null || value === undefined) return null;
	const { max } = scale(format);
	return Math.max(0, Math.min(100, (value / max) * 100));
}

const SMILEYS = ['', '🙁', '😐', '🙂'];

export function formatScore(value: number | null, format: ScoreFormat): string {
	const shown = toDisplay(value, format);
	if (shown === null) return '–';

	switch (format) {
		case 'pt100':
			return String(Math.round(shown));
		case 'pt10dec':
			return shown.toFixed(1);
		case 'pt10':
			return String(Math.round(shown));
		case 'star5': {
			const full = Math.floor(shown);
			const half = shown - full >= 0.5;
			return '★'.repeat(full) + (half ? '½' : '') || '–';
		}
		case 'smiley3':
			return SMILEYS[Math.max(1, Math.min(3, Math.round(shown)))] ?? '–';
	}
}

export function runtime(minutes: number | null | undefined): string {
	if (!minutes || minutes <= 0) return '';
	const h = Math.floor(minutes / 60);
	const m = minutes % 60;
	if (!h) return `${m}m`;
	return m ? `${h}h ${m}m` : `${h}h`;
}

/** Big totals read better as days and hours than as 143,220 minutes. */
export function longRuntime(minutes: number): string {
	if (!minutes) return '0m';
	const days = Math.floor(minutes / 1440);
	const hours = Math.floor((minutes % 1440) / 60);
	if (days >= 1) return `${days}d ${hours}h`;
	return runtime(minutes);
}

export function year(date: string | null | undefined): string {
	return date?.slice(0, 4) ?? '';
}

export function prettyDate(date: string | null | undefined): string {
	if (!date) return '';
	const d = new Date(date);
	if (Number.isNaN(d.getTime())) return '';
	return d.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });
}

export function relativeDate(iso: string | null | undefined): string {
	if (!iso) return '';
	const then = new Date(iso).getTime();
	if (Number.isNaN(then)) return '';

	const diff = Date.now() - then;
	const day = 86_400_000;
	if (diff < 0) {
		const days = Math.ceil(-diff / day);
		if (days === 0) return 'today';
		if (days === 1) return 'tomorrow';
		if (days < 30) return `in ${days} days`;
		return prettyDate(iso);
	}
	if (diff < day) return 'today';
	if (diff < 2 * day) return 'yesterday';
	if (diff < 30 * day) return `${Math.floor(diff / day)} days ago`;
	return prettyDate(iso);
}

export function episodeCode(season: number, episode: number): string {
	const pad = (n: number) => String(n).padStart(2, '0');
	return `S${pad(season)}E${pad(episode)}`;
}
