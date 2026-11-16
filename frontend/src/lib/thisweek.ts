import { episodeCode } from './format';
import type { AiringEntry } from './types';

/** The earliest moment on the entry's first day. The first episode may be a
 * special no provider times while the episode beside it has a time. */
export function firstMoment(entry: AiringEntry): string | null {
	const moments = entry.episodes
		.filter((e) => e.air_date === entry.air_date && e.air_at)
		.map((e) => e.air_at as string)
		.sort((a, b) => new Date(a).getTime() - new Date(b).getTime());
	return moments[0] ?? null;
}

/** "S01E05" for one, "S01E05–E06" for a run of them. A special that airs
 * beside regular episodes counts towards "2 episodes" but not the range,
 * which would otherwise read "S00E07–S01E07". */
export function episodeRange(entry: AiringEntry): string {
	const regular = entry.episodes.filter((e) => e.season_number > 0);
	const run = regular.length ? regular : entry.episodes;
	const first = run[0];
	const last = run[run.length - 1];
	const start = episodeCode(first.season_number, first.episode_number);
	if (run.length === 1) return start;
	const end =
		last.season_number === first.season_number
			? `E${String(last.episode_number).padStart(2, '0')}`
			: episodeCode(last.season_number, last.episode_number);
	return `${start}–${end}`;
}
