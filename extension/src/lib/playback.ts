// Which video is the one being watched, and when it counts as watched.
// Plain functions, so they are tested without a browser.

/** Anything shorter is a trailer, a clip or an ad. */
export const MIN_SECONDS = 180;

export interface VideoSample {
	/** The element, unique to this page load: a reload is a new sitting. */
	id: string;
	src: string;
	duration: number;
	position: number;
	playing: boolean;
	ended: boolean;
	/** Visible pixels. */
	area: number;
	fullscreen: boolean;
}

/** Long enough to be a film or an episode, and a real, finite length. */
export const counts = (v: Pick<VideoSample, 'duration'>) =>
	Number.isFinite(v.duration) && v.duration >= MIN_SECONDS;

/** The biggest video that is actually playing (or just ended) and long enough. */
export function pick(videos: VideoSample[]): VideoSample | null {
	let best: VideoSample | null = null;
	for (const v of videos) {
		if (!counts(v) || (!v.playing && !v.ended) || v.area <= 0) continue;
		if (!best || v.area > best.area) best = v;
	}
	return best;
}

/**
 * What one sitting is: one element, one source, one length. A pre-roll ad that
 * hands the element over to the episode changes the source or the length, and
 * a player that swaps its element changes the element, so either starts a new
 * sitting and nothing the ad did counts.
 */
export function sittingKey(v: Pick<VideoSample, 'id' | 'src' | 'duration'>): string {
	return `${v.id}|${v.src}|${Math.round(v.duration / 5)}`;
}

/** Whether a sitting has gone far enough to log. */
export function watched(
	position: number,
	duration: number,
	ended: boolean,
	threshold: number
): boolean {
	if (!counts({ duration })) return false;
	if (ended) return true;
	return position / duration >= Math.min(Math.max(threshold, 50), 100) / 100;
}
