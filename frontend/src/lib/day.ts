/** The day as the person looking at it would write it, not as UTC has it. */
export function localDay(at: Date = new Date()): string {
	const pad = (n: number) => String(n).padStart(2, '0');
	return `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}`;
}

/**
 * The day something comes out, where the viewer is. A moment from TheTVDB
 * (`air_at`) can fall on the day before or after the network's own date;
 * without one, the stored day stands.
 */
export function airDay(date: string | null, at?: string | null): string | null {
	if (!at) return date;
	const moment = new Date(at);
	return Number.isNaN(moment.getTime()) ? date : localDay(moment);
}
