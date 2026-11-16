/**
 * Badges Frog gives on its own climb ladders (10 films, 50, 100…). Beside a
 * name only the highest step of each ladder shows, where the lower ones
 * were; badges on no ladder stay as they are.
 */

export interface OnLadder {
	ladder?: string | null;
	step?: number | null;
}

export function topSteps<T extends OnLadder>(badges: T[]): T[] {
	const top = new Map<string, number>();
	for (const b of badges) {
		if (b.ladder == null || b.step == null) continue;
		top.set(b.ladder, Math.max(top.get(b.ladder) ?? -1, b.step));
	}
	const placed = new Set<string>();
	const out: T[] = [];
	for (const b of badges) {
		if (b.ladder == null || b.step == null) {
			out.push(b);
			continue;
		}
		const ladder = b.ladder;
		if (placed.has(ladder)) continue;
		placed.add(ladder);
		out.push(badges.find((x) => x.ladder === ladder && x.step === top.get(ladder)) ?? b);
	}
	return out;
}
