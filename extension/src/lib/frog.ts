// Talking to Frog: the four extension endpoints and nothing else. Only the
// background script calls these, with the token in a header.

import type { Signals } from './signals.ts';
import type { Me, Resolution, Settings } from './types.ts';

export class FrogError extends Error {
	readonly status: number;
	constructor(message: string, status: number) {
		super(message);
		this.name = 'FrogError';
		this.status = status;
	}
}

async function call<T>(s: Settings, path: string, body?: unknown): Promise<T> {
	if (!s.url || !s.token) throw new FrogError('Connect to Frog first.', 401);
	let res: Response;
	try {
		res = await fetch(`${s.url}/api/ext/${path}`, {
			method: body === undefined ? 'GET' : 'POST',
			headers: {
				authorization: `Bearer ${s.token}`,
				...(body === undefined ? {} : { 'content-type': 'application/json' })
			},
			body: body === undefined ? undefined : JSON.stringify(body),
			credentials: 'omit',
			signal: AbortSignal.timeout(15_000)
		});
	} catch {
		throw new FrogError('Frog cannot be reached.', 0);
	}
	const text = await res.text();
	let parsed: unknown = null;
	try {
		parsed = text ? JSON.parse(text) : null;
	} catch {
		parsed = null;
	}
	if (!res.ok) {
		const said = (parsed as { error?: string } | null)?.error;
		const message =
			res.status === 401
				? 'Frog did not accept the token. Make a new one in Settings → Connections.'
				: res.status === 429
					? 'Too many tries. Wait a few minutes.'
					: (said ?? `Frog answered ${res.status}.`);
		throw new FrogError(message, res.status);
	}
	return parsed as T;
}

export const frog = {
	me: (s: Settings) => call<Me>(s, 'me'),
	resolve: (s: Settings, signals: Signals) => call<Resolution>(s, 'resolve', signals),
	log: (
		s: Settings,
		body: {
			title_id?: number;
			tmdb_id?: number;
			kind?: string;
			season?: number;
			episode?: number;
			watched_at: string;
		}
	) =>
		call<{ log_id: number; already: boolean; name: string; status: string | null }>(s, 'log', body),
	undo: (s: Settings, logId: number) => call<{ ok: true }>(s, 'undo', { log_id: logId })
};
