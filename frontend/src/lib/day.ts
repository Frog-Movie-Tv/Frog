/** The day as the person looking at it would write it, not as UTC has it. */
export function localDay(at: Date = new Date()): string {
	const pad = (n: number) => String(n).padStart(2, '0');
	return `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}`;
}
