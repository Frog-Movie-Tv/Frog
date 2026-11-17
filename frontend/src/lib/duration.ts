/**
 * "2h 15m": a length in minutes as Frog writes it. Apart from `format.ts`,
 * which reads settings, so pure modules and their tests can use it.
 */
export function runtime(minutes: number | null | undefined): string {
	if (!minutes || minutes <= 0) return '';
	const h = Math.floor(minutes / 60);
	const m = minutes % 60;
	if (!h) return `${m}m`;
	return m ? `${h}h ${m}m` : `${h}h`;
}
