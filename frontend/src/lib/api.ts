import { airDay, viewerZone } from './day';
import { settings } from './settings.svelte';
import type { TitleRef } from './titles';
import type { WhenAnswer } from './when.svelte';
import type {
	TitleMerge,
	StatsNarrow,
	ActivityItem,
	AiringDay,
	AiringFilters,
	AiringWeek,
	AniListChoice,
	AniListEntry,
	AniListStatus,
	MoreResults,
	SearchAnswer,
	SearchFilters,
	TitleScores,
	StudioDetail,
	AuthStatus,
	BecauseRow,
	HomeData,
	ImportSummary,
	Account,
	AdminUser,
	DeviceSession,
	InstanceInfo,
	ServerStatus,
	Invite,
	PersonSummary,
	Profile,
	ProfileEdit,
	NewInvite,
	ResetLink,
	MatchResult,
	Kind,
	LibraryItem,
	SharedLibrary,
	PersonDetail,
	SearchItem,
	SeasonDetail,
	FullStats,
	MangaStats,
	EntryRecord,
	Shelf,
	ShelfRules,
	Stats,
	Status,
	TitleDetail,
	TitleHistory,
	RatedTitle,
	Comparison
} from './types';

export class ApiError extends Error {
	constructor(
		message: string,
		readonly status: number,
		readonly twoFactor = false
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
	} catch (e) {
		// Called off: a newer request took its place, and nobody is waiting.
		if (init?.signal?.aborted) throw e;
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
		const twoFactor = !!body && typeof body === 'object' && 'two_factor' in body;
		throw new ApiError(detail || `Request failed (${res.status})`, res.status, twoFactor);
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

const get = <T>(path: string, signal?: AbortSignal) =>
	request<T>(path, signal ? { signal } : undefined);

/** A request called off because a newer one replaced it: not an error. */
export const isAbort = (e: unknown) => e instanceof DOMException && e.name === 'AbortError';
const send = <T>(method: string, path: string, body?: unknown) =>
	request<T>(path, { method, body: body === undefined ? undefined : JSON.stringify(body) });

/** One step down for each size, used when data saver is on. */
const SMALLER: Record<string, string> = {
	original: 'w1280',
	w1280: 'w780',
	w780: 'w500',
	w500: 'w342',
	w342: 'w185',
	w300: 'w185',
	w185: 'w154',
	w154: 'w92'
};

/** Image proxy. Every poster goes through our own origin, never TMDB directly. */
export function img(path: string | null | undefined, size = 'w342'): string | null {
	if (!path) return null;
	const chosen = settings.current.dataSaver ? (SMALLER[size] ?? size) : size;
	return `/img/${chosen}${path.startsWith('/') ? path : `/${path}`}`;
}

/** One reading of the library: the titles, and what its filters have to offer. */
export interface LibraryAnswer {
	items: LibraryItem[];
	/** What each status would show with the other filters as they are. */
	counts: Partial<Record<Status, number>>;
	/** Every genre in the library, whatever is filtered. */
	genres: string[];
}

export interface LibraryQuery {
	status?: Status;
	kind?: Kind;
	genre?: string;
	q?: string;
	favourite?: boolean;
	rewatched?: boolean;
	sort?: 'added' | 'updated' | 'name' | 'rating' | 'release' | 'last_watched';
	order?: 'asc' | 'desc';
	limit?: number;
	offset?: number;
}

type Picked = { title_ids: number[] };

/** What Library's edit mode can do to the titles it has ticked. */
/** One watch of an episode as unticking it cleared it. */
export interface ClearedWatch {
	watched_at: string;
	undated: boolean;
	is_rewatch: boolean;
	source: string;
}

export type BulkAction =
	| ({ action: 'remove' } & Picked)
	| { action: 'restore'; entries: EntryRecord[] }
	| { action: 'unmark'; entries: EntryRecord[] }
	| ({ action: 'watched' } & Picked & WhenAnswer)
	| ({ action: 'status'; status: 'completed' } & Picked & WhenAnswer)
	| ({ action: 'status'; status: Exclude<Status, 'completed'> } & Picked)
	| ({ action: 'favourite'; favourite: boolean } & Picked)
	| ({ action: 'shelf'; shelf_id: number } & Picked);

interface Marked {
	marked: number;
	/** Nothing out yet to have watched. */
	skipped: number;
	/** The titles as they were, for `unmark`. */
	before: EntryRecord[];
}

/** What each action answers. `before` is what undo sends back. */
type BulkAnswer<A extends BulkAction['action'], Finishing extends boolean> = A extends 'watched'
	? Marked
	: A extends 'status'
		? Finishing extends true
			? Marked
			: { before: EntryRecord[] }
		: A extends 'restore' | 'unmark'
			? { restored: number }
			: A extends 'shelf'
				? { added: number }
				: { before: EntryRecord[] };

/** Home's days as they fall where the viewer is, for anything with a time. */
function localHome(home: HomeData): HomeData {
	for (const entry of [home.hero, ...home.continue, ...home.waiting]) {
		if (entry?.next) entry.next.air_date = airDay(entry.next.air_date, entry.next.air_at);
	}
	for (const airing of home.this_week) {
		for (const ep of airing.episodes) ep.air_date = airDay(ep.air_date, ep.air_at);
		airing.air_date = airing.episodes[0]?.air_date ?? airing.air_date;
	}
	return home;
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
	login: (username: string, password: string, remember: boolean, code?: string) =>
		send<{ ok: true }>('POST', '/api/auth/login', { username, password, remember, code }),
	twoFactor: () => get<{ enabled: boolean; recovery_codes_left: number }>('/api/auth/two-factor'),
	startTwoFactor: () =>
		send<{ secret: string; uri: string; qr_svg: string }>('POST', '/api/auth/two-factor/start', {}),
	confirmTwoFactor: (code: string) =>
		send<{ recovery_codes: string[] }>('POST', '/api/auth/two-factor/confirm', { code }),
	disableTwoFactor: (password: string) =>
		send<{ ok: true }>('POST', '/api/auth/two-factor/disable', { password }),
	newRecoveryCodes: (password: string) =>
		send<{ recovery_codes: string[] }>('POST', '/api/auth/two-factor/recovery-codes', {
			password
		}),
	logout: () => send<{ ok: true }>('POST', '/api/auth/logout', {}),

	/** Empties this account's library. There is no undo. */
	clearLibrary: (password: string, confirm: string) =>
		send<{ titles: number; watches: number; shelves: number }>('DELETE', '/api/account/library', {
			password,
			confirm
		}),

	/** Joining with an invite link. There is no open sign-up. */
	register: (username: string, password: string, invite: string, remember = true) =>
		send<{ ok: true }>('POST', '/api/auth/register', { username, password, invite, remember }),

	/** A new password from a reset link. Signs nobody in. */
	resetPassword: (code: string, password: string) =>
		send<{ ok: true }>('POST', '/api/auth/reset', { code, password }),

	account: () => get<Account>('/api/account'),
	changeUsername: (username: string, password: string) =>
		send<{ username: string }>('PUT', '/api/account/username', { username, password }),
	changePassword: (current: string, password: string) =>
		send<{ ok: true; signed_out: number }>('PUT', '/api/account/password', { current, password }),
	sessions: () => get<{ sessions: DeviceSession[] }>('/api/account/sessions'),
	/** Titles whose data was moved onto another, newest first. */
	merges: () => get<{ merges: TitleMerge[] }>('/api/account/merges'),
	undoMerge: (id: number) => send<{ ok: true }>('POST', `/api/account/merges/${id}/undo`),
	endSession: (id: string) => send<{ ok: true }>('DELETE', `/api/account/sessions/${id}`),
	endOtherSessions: () => send<{ signed_out: number }>('DELETE', '/api/account/sessions/others'),
	setProfilePublic: (value: boolean) =>
		send<Account>('PUT', '/api/account/profile', { public: value }),
	updateProfile: (edit: ProfileEdit) => send<Account>('PUT', '/api/account/profile', edit),
	/** The picture goes up as it is; the server checks what it really is. */
	uploadProfileImage: (kind: 'avatar' | 'banner', file: Blob) =>
		request<{ ok: true }>(`/api/account/images/${kind}`, {
			method: 'PUT',
			body: file,
			headers: { 'content-type': file.type || 'application/octet-stream' }
		}),
	removeProfileImage: (kind: 'avatar' | 'banner') =>
		send<{ ok: true }>('DELETE', `/api/account/images/${kind}`),
	deleteAccount: (password: string) => send<{ ok: true }>('DELETE', '/api/account', { password }),

	adminUsers: () => get<{ users: AdminUser[] }>('/api/admin/users'),
	updateUser: (id: number, patch: { role?: 'admin' | 'user'; disabled?: boolean }) =>
		send<{ id: number; role: AdminUser['role']; disabled: boolean }>(
			'PUT',
			`/api/admin/users/${id}`,
			patch
		),
	deleteUser: (id: number) => send<{ ok: true }>('DELETE', `/api/admin/users/${id}`),
	instance: () => get<InstanceInfo>('/api/admin/instance'),
	server: () => get<ServerStatus>('/api/admin/server'),

	anilist: () => get<AniListStatus>('/api/anilist'),
	anilistConnect: () => send<{ url: string }>('POST', '/api/anilist/connect', {}),
	anilistSync: () => send<{ syncing: true }>('POST', '/api/anilist/sync', {}),
	anilistDisconnect: () => send<{ ok: true }>('DELETE', '/api/anilist'),
	anilistEntries: () => get<{ entries: AniListEntry[] }>('/api/anilist/entries'),
	anilistChoose: (mediaId: number, choice: AniListChoice) =>
		send<{ ok: true }>('PUT', `/api/anilist/entries/${mediaId}`, choice),

	profiles: () => get<{ profiles: PersonSummary[] }>('/api/profiles'),
	profile: (username: string) => get<Profile>(`/api/profiles/${encodeURIComponent(username)}`),
	profileRatings: (username: string) =>
		get<{ ratings: RatedTitle[] }>(`/api/profiles/${encodeURIComponent(username)}/ratings`),
	profileCompare: (username: string) =>
		get<Comparison>(`/api/profiles/${encodeURIComponent(username)}/compare`),
	profileLibrary: (username: string, query: LibraryQuery) =>
		get<SharedLibrary>(
			`/api/profiles/${encodeURIComponent(username)}/library${qs(query as Record<string, unknown>)}`
		),

	invites: () => get<{ invites: Invite[] }>('/api/invites'),
	createResetLink: (id: number) => send<ResetLink>('POST', `/api/admin/users/${id}/reset`, {}),
	createInvite: (role: Invite['role'], days?: number) =>
		send<NewInvite>('POST', '/api/invites', { role, days }),
	revokeInvite: (id: string) => send<{ ok: true }>('DELETE', `/api/invites/${id}`),

	search: (
		q: string,
		page = 1,
		hideAnimation = false,
		filters: SearchFilters = {},
		signal?: AbortSignal
	) =>
		get<SearchAnswer>(
			`/api/search${qs({
				q,
				page,
				hide_animation: hideAnimation || undefined,
				kind: filters.kind,
				genres: filters.genres?.join(','),
				from_year: filters.from_year,
				to_year: filters.to_year,
				min_score: filters.min_score,
				statuses: filters.statuses?.join(','),
				sort: filters.sort
			})}`,
			signal
		),
	searchGenres: () => get<{ genres: string[] }>('/api/search/genres'),
	scores: (id: number) => get<TitleScores>(`/api/t/${id}/scores`),
	trending: (list: 'trending' | 'top' = 'trending', page = 1) =>
		get<{ results: SearchItem[]; next_page?: number | null }>(
			`/api/discover/trending${qs({ list: list === 'top' ? 'top' : undefined, page: page > 1 ? page : undefined })}`
		),
	/** People, studios, characters and manga; it asks AniList, which takes
	 *  one call at a time, so it is asked once typing pauses (`MORE_PAUSE`). */
	searchMore: (q: string, signal?: AbortSignal) =>
		get<MoreResults>(`/api/search/more${qs({ q })}`, signal),
	studio: (id: number, page = 1) =>
		get<StudioDetail>(`/api/studios/${id}${qs({ page: page > 1 ? page : undefined })}`),

	home: () => get<HomeData>('/api/home').then(localHome),
	because: () => get<{ rows: BecauseRow[] }>('/api/home/because'),
	/** More like a title, a page at a time: what "View more" lists. */
	moreLike: (titleId: number, page: number) =>
		get<{ seed: { id: number; name: string }; items: SearchItem[]; next_page: number | null }>(
			`/api/t/${titleId}/more${qs({ page })}`
		),
	activity: () => get<{ items: ActivityItem[] }>('/api/home/activity'),
	/** Everyone's activity, all of it, a page before `before` at a time. */
	allActivity: (before?: string | null) =>
		get<{ items: ActivityItem[]; next: string | null }>(
			`/api/activity${qs({ before: before ?? undefined })}`
		),

	matchImport: (items: unknown[]) =>
		send<{ results: MatchResult[] }>('POST', '/api/import/match', { items }),

	applyImport: (items: unknown[], onConflict: 'skip' | 'overwrite' | 'merge') =>
		send<ImportSummary>('POST', '/api/import/apply', { items, on_conflict: onConflict }),

	library: (query: LibraryQuery = {}) =>
		get<LibraryAnswer>(`/api/library${qs(query as Record<string, unknown>)}`),
	/** One action on many titles, in one request. */
	bulk: <B extends BulkAction>(body: B) =>
		send<BulkAnswer<B['action'], B extends { status: 'completed' } ? true : false>>(
			'POST',
			'/api/library/bulk',
			body
		),
	stats: () => get<Stats>('/api/library/stats'),
	shelves: () => get<{ shelves: Shelf[] }>('/api/shelves'),
	shelf: (id: number) => get<{ shelf: Shelf; items: LibraryItem[] }>(`/api/shelves/${id}`),
	createShelf: (body: {
		name: string;
		kind: Shelf['kind'];
		rules?: ShelfRules;
		title_ids?: number[];
	}) => send<{ shelf: Shelf }>('POST', '/api/shelves', body),
	updateShelf: (id: number, body: { name?: string; rules?: ShelfRules }) =>
		send<{ shelf: Shelf }>('PUT', `/api/shelves/${id}`, body),
	/** Returns what it takes to put the shelf back. */
	deleteShelf: (id: number) =>
		send<{ name: string; kind: Shelf['kind']; rules: ShelfRules; title_ids: number[] }>(
			'DELETE',
			`/api/shelves/${id}`
		),
	reorderShelves: (ids: number[]) => send<{ ok: true }>('PUT', '/api/shelves/order', { ids }),
	addToShelf: (id: number, titleId: number) =>
		send<{ ok: true }>('POST', `/api/shelves/${id}/items`, { title_id: titleId }),
	removeFromShelf: (id: number, titleId: number) =>
		send<{ ok: true }>('DELETE', `/api/shelves/${id}/items/${titleId}`),
	reorderShelf: (id: number, titleIds: number[]) =>
		send<{ ok: true }>('PUT', `/api/shelves/${id}/items/order`, { title_ids: titleIds }),
	titleShelves: (titleId: number) =>
		get<{ shelves: { id: number; name: string; on: boolean }[] }>(`/api/t/${titleId}/shelves`),
	pushStatus: () => get<{ public_key: string; devices: number }>('/api/push'),
	/** With this browser's zone, which says when midnight is for a day with no time. */
	pushSubscribe: (subscription: PushSubscriptionJSON) =>
		send<{ ok: true }>('POST', '/api/push/subscribe', {
			...subscription,
			time_zone: viewerZone()
		}),
	pushUnsubscribe: (endpoint: string) =>
		send<{ ok: true }>('POST', '/api/push/unsubscribe', { endpoint }),
	/** Seven days of the Airing page from `from`, a day's first shows each. */
	airing: (filters: AiringFilters, from?: string) =>
		get<AiringWeek>(`/api/airing${qs({ ...filters, from, tz: viewerZone() })}`),
	/** More of one Airing day, for "Show more". */
	airingDay: (filters: AiringFilters, date: string, offset: number) =>
		get<AiringDay>(`/api/airing/day${qs({ ...filters, date, offset, tz: viewerZone() })}`),
	fullStats: (year?: number) => get<FullStats>(`/api/stats${qs({ year })}`),
	/** Days, streaks and people for the titles a pick on Stats keeps. */
	statsNarrow: (half: 'watching' | 'manga', ids: number[], year?: number, unit?: string) =>
		get<StatsNarrow>(`/api/stats/narrow${qs({ half, year, unit, ids: ids.join(',') })}`),
	mangaStats: (year?: number) => get<MangaStats>(`/api/stats/manga${qs({ year })}`),

	/** A title as a provider names it (`tv/1399`, `anilist/21`): opens it, and the answer carries Frog's id. */
	titleAt: (kind: Kind | 'anilist', id: number) => get<TitleDetail>(`/api/titles/${kind}/${id}`),
	/** A title by Frog's id. Everything below goes by it too. */
	title: (titleId: number) =>
		get<TitleDetail>(`/api/t/${titleId}`).then((t) => {
			if (t.next_up) t.next_up.air_date = airDay(t.next_up.air_date, t.next_up.air_at);
			return t;
		}),
	season: (titleId: number, season: number) =>
		get<SeasonDetail>(`/api/t/${titleId}/seasons/${season}`).then((s) => {
			for (const ep of s.episodes) ep.air_date = airDay(ep.air_date, ep.air_at);
			return s;
		}),
	person: (id: number) => get<PersonDetail>(`/api/person/${id}`),

	track: (
		titleId: number,
		patch: {
			status?: Status;
			rating?: number | null;
			favourite?: boolean;
			notes?: string | null;
			notify?: boolean;
			/** Chapters read, for a manga. */
			progress?: number;
		}
	) => send<{ item: LibraryItem }>('PUT', `/api/t/${titleId}/library`, patch),

	untrack: (titleId: number) => send<{ ok: true }>('DELETE', `/api/t/${titleId}/library`),

	/** Watched it again: another watch of a film, another time through a show. */
	rewatch: (titleId: number, body: { watched_at?: string; undated?: boolean }) =>
		send<{ history: TitleHistory | null }>('POST', `/api/t/${titleId}/rewatch`, body),

	/**
	 * `at` puts back a watch exactly as it was, for undo; `restore` puts back
	 * every watch an untick cleared, which it answers with.
	 */
	setEpisodeWatched: (
		titleId: number,
		season: number,
		episode: number,
		watched: boolean,
		at?: { watched_at: string | null; undated: boolean },
		restore?: ClearedWatch[]
	) =>
		send<{ item: LibraryItem; cleared?: ClearedWatch[] }>(
			'POST',
			`/api/t/${titleId}/episodes/${season}/${episode}/watch`,
			{
				watched,
				watched_at: at?.watched_at ?? undefined,
				undated: at?.undated ?? false,
				restore: restore ?? []
			}
		),

	setSeasonWatched: (titleId: number, season: number, watched: boolean, when?: WhenAnswer) =>
		send<{ item: LibraryItem }>('POST', `/api/t/${titleId}/episodes/${season}/watch`, {
			watched,
			...when
		}),

	setShowWatched: (titleId: number, watched: boolean, when?: WhenAnswer) =>
		send<{ item: LibraryItem }>('POST', `/api/t/${titleId}/episodes/watch`, {
			watched,
			...when,
			extras: undefined,
			specials: when?.extras
		}),

	redateDiary: (titleId: number, day: string, when: WhenAnswer) =>
		send<{ changed: number }>('POST', '/api/diary/redate', { title_id: titleId, day, ...when }),

	rateSeason: (titleId: number, season: number, firstEpisode: number, rating: number | null) =>
		send<{ ok: true }>('PUT', `/api/t/${titleId}/seasons/${season}/rating`, {
			rating,
			first_episode: firstEpisode
		}),

	setEpisodeState: (
		titleId: number,
		season: number,
		episode: number,
		state: { rating: number | null; notes: string | null }
	) => send<{ ok: true }>('PUT', `/api/t/${titleId}/episodes/${season}/${episode}/state`, state)
};

/**
 * How long a search field waits after a key before it asks: long enough that
 * a quick burst of keys asks once, short enough that results follow typing.
 */
export const SEARCH_PAUSE = 60;

/**
 * The same for a search that reaches AniList (manga, characters). AniList is
 * asked one call at a time, so a call a key would queue behind the keys
 * before it and answer long after; this asks once typing pauses.
 */
export const MORE_PAUSE = 350;

/**
 * Frog's id for a title, opening it by its provider's address first if Frog
 * does not have it yet (a search result, a credit).
 */
export async function frogId(t: TitleRef): Promise<number> {
	if (t.id) return t.id;
	if (t.kind && t.tmdb_id) return (await api.titleAt(t.kind, t.tmdb_id)).item.id;
	if (t.anilist_id) return (await api.titleAt('anilist', t.anilist_id)).item.id;
	throw new Error('That title cannot be found.');
}
