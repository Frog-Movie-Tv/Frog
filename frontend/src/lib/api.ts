import type {
	AuthStatus,
	HomeData,
	ImportSummary,
	MatchResult,
	Kind,
	LibraryItem,
	PersonDetail,
	SearchItem,
	SeasonDetail,
	Stats,
	Status,
	TitleDetail
} from './types';

export class ApiError extends Error {
	constructor(
		message: string,
		readonly status: number
	) {
		super(message);
		this.name = 'ApiError';
	}

	get isAuth() {
		return this.status === 401;
	}
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
	let res: Response;
	try {
		res = await fetch(path, {
			credentials: 'same-origin',
			...init,
			headers: {
				...(init?.body ? { 'content-type': 'application/json' } : {}),
				...init?.headers
			}
		});
	} catch {
		// Offline, or the backend is down. The caller decides whether to fall
		// back to cached data or show the banner.
		throw new ApiError('Cannot reach Frog. Are you on the right network?', 0);
	}

	if (res.status === 204) return undefined as T;

	const text = await res.text();
	const body = text ? safeParse(text) : null;

	if (!res.ok) {
		const detail =
			body && typeof body === 'object' && 'error' in body
				? String((body as { error: unknown }).error)
				: '';
		throw new ApiError(detail || `Request failed (${res.status})`, res.status);
	}

	return body as T;
}

function safeParse(text: string): unknown {
	try {
		return JSON.parse(text);
	} catch {
		return null;
	}
}

const get = <T>(path: string) => request<T>(path);
const send = <T>(method: string, path: string, body?: unknown) =>
	request<T>(path, { method, body: body === undefined ? undefined : JSON.stringify(body) });

/** Image proxy. Every poster goes through our own origin, never TMDB directly. */
export function img(path: string | null | undefined, size = 'w342'): string | null {
	if (!path) return null;
	return `/img/${size}${path.startsWith('/') ? path : `/${path}`}`;
}

export interface LibraryQuery {
	status?: Status;
	kind?: Kind;
	genre?: string;
	q?: string;
	favourite?: boolean;
	sort?: 'added' | 'updated' | 'name' | 'rating' | 'release' | 'last_watched';
	order?: 'asc' | 'desc';
	limit?: number;
}

function qs(params: Record<string, unknown>): string {
	const search = new URLSearchParams();
	for (const [k, v] of Object.entries(params)) {
		if (v !== undefined && v !== null && v !== '') search.set(k, String(v));
	}
	const s = search.toString();
	return s ? `?${s}` : '';
}

export const api = {
	status: () => get<AuthStatus>('/api/auth/status'),
	setup: (username: string, password: string) =>
		send<{ ok: true }>('POST', '/api/auth/setup', { username, password }),
	login: (username: string, password: string) =>
		send<{ ok: true }>('POST', '/api/auth/login', { username, password }),
	logout: () => send<{ ok: true }>('POST', '/api/auth/logout', {}),

	search: (q: string, page = 1, hideAnimation = false) =>
		get<{ page: number; total_pages: number; results: SearchItem[] }>(
			`/api/search${qs({ q, page, hide_animation: hideAnimation || undefined })}`
		),
	trending: () => get<{ results: SearchItem[] }>('/api/discover/trending'),

	home: () => get<HomeData>('/api/home'),

	matchImport: (items: unknown[]) =>
		send<{ results: MatchResult[] }>('POST', '/api/import/match', { items }),

	applyImport: (items: unknown[], onConflict: 'skip' | 'overwrite') =>
		send<ImportSummary>('POST', '/api/import/apply', { items, on_conflict: onConflict }),

	library: (query: LibraryQuery = {}) =>
		get<{ items: LibraryItem[] }>(`/api/library${qs(query as Record<string, unknown>)}`),
	stats: () => get<Stats>('/api/library/stats'),

	title: (kind: Kind, id: number) => get<TitleDetail>(`/api/titles/${kind}/${id}`),
	season: (kind: Kind, id: number, season: number) =>
		get<SeasonDetail>(`/api/titles/${kind}/${id}/seasons/${season}`),
	person: (id: number) => get<PersonDetail>(`/api/person/${id}`),

	track: (
		kind: Kind,
		id: number,
		patch: { status?: Status; rating?: number | null; favourite?: boolean; notes?: string | null }
	) => send<{ item: LibraryItem }>('PUT', `/api/titles/${kind}/${id}/library`, patch),

	untrack: (kind: Kind, id: number) =>
		send<{ ok: true }>('DELETE', `/api/titles/${kind}/${id}/library`),

	setEpisodeWatched: (kind: Kind, id: number, season: number, episode: number, watched: boolean) =>
		send<{ item: LibraryItem }>(
			'POST',
			`/api/titles/${kind}/${id}/episodes/${season}/${episode}/watch`,
			{ watched }
		),

	setSeasonWatched: (kind: Kind, id: number, season: number, watched: boolean) =>
		send<{ item: LibraryItem }>('POST', `/api/titles/${kind}/${id}/episodes/${season}/watch`, {
			watched
		}),

	setEpisodeState: (
		kind: Kind,
		id: number,
		season: number,
		episode: number,
		state: { rating: number | null; notes: string | null }
	) =>
		send<{ ok: true }>(
			'PUT',
			`/api/titles/${kind}/${id}/episodes/${season}/${episode}/state`,
			state
		)
};
