// The decisions behind the queue of watches on their way to Frog. A watch is
// only ever removed once Frog has it, or once you discard it yourself.

import type { Pending } from './types.ts';

/** Minutes to wait before the next try: 1, 2, 4 … up to an hour. */
export function backoff(tries: number): number {
	return Math.min(2 ** Math.max(tries - 1, 0), 60) * 60_000;
}

export type Outcome = 'done' | 'retry' | 'reconnect' | 'refused';

/** What an answer from Frog means for a queued watch. */
export function outcome(status: number): Outcome {
	if (status >= 200 && status < 300) return 'done';
	if (status === 401 || status === 403) return 'reconnect';
	// Throttled, timed out, or Frog having a bad moment: try again later.
	if (status === 0 || status === 408 || status === 429 || status >= 500) return 'retry';
	// Frog will never take it as it is (an episode it does not have). It stays
	// in the queue, marked, until you retry or discard it.
	return 'refused';
}

/** Those due now, oldest first; refused ones wait for you. */
export function due(queue: Pending[], now: number): Pending[] {
	return queue.filter((p) => !p.failed && p.next <= now).sort((a, b) => a.next - b.next);
}

/** What a watch is called in a toast or the popup. */
export function label(name: string, season?: number | null, episode?: number | null): string {
	if (episode == null) return name;
	return season == null ? `${name} episode ${episode}` : `${name} S${season}E${episode}`;
}
