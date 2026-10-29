import { settings } from './settings.svelte';
import type { WhenAnswer } from './when.svelte';
import type {
	AniListChoice,
	AniListEntry,
	AniListStatus,
	MoreResults,
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
	MatchResult,
	Kind,
	LibraryItem,
	PersonDetail,
	SearchItem,
	SeasonDetail,
	FullStats,
	Shelf,
	ShelfRules,
	Stats,
	Status,
	TitleDetail
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

const get = <T>(path: string) => request<T>(path);
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

	account: () => get<Account>('/api/account'),
	changePassword: (current: string, password: string) =>
		send<{ ok: true; signed_out: number }>('PUT', '/api/account/password', { current, password }),
	sessions: () => get<{ sessions: DeviceSession[] }>('/api/account/sessions'),
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

	invites: () => get<{ invites: Invite[] }>('/api/invites'),
	createInvite: (role: Invite['role'], days?: number) =>
		send<NewInvite>('POST', '/api/invites', { role, days }),
	revokeInvite: (id: string) => send<{ ok: true }>('DELETE', `/api/invites/${id}`),

	search: (q: string, page = 1, hideAnimation = false) =>
		get<{ page: number; total_pages: number; results: SearchItem[] }>(
			`/api/search${qs({ q, page, hide_animation: hideAnimation || undefined })}`
		),
	trending: () => get<{ results: SearchItem[] }>('/api/discover/trending'),
	searchMore: (q: string) => get<MoreResults>(`/api/search/more${qs({ q })}`),
	studio: (id: number) => get<StudioDetail>(`/api/studios/${id}`),

	home: () => get<HomeData>('/api/home'),
	because: () => get<{ rows: BecauseRow[] }>('/api/home/because'),

	matchImport: (items: unknown[]) =>
		send<{ results: MatchResult[] }>('POST', '/api/import/match', { items }),

	applyImport: (items: unknown[], onConflict: 'skip' | 'overwrite' | 'merge') =>
		send<ImportSummary>('POST', '/api/import/apply', { items, on_conflict: onConflict }),

	library: (query: LibraryQuery = {}) =>
		get<{ items: LibraryItem[] }>(`/api/library${qs(query as Record<string, unknown>)}`),
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
	addToShelf: (id: number, kind: Kind, tmdbId: number) =>
		send<{ ok: true }>('POST', `/api/shelves/${id}/items`, { kind, tmdb_id: tmdbId }),
	removeFromShelf: (id: number, kind: Kind, tmdbId: number) =>
		send<{ ok: true }>('DELETE', `/api/shelves/${id}/items/${kind}/${tmdbId}`),
	reorderShelf: (id: number, titleIds: number[]) =>
		send<{ ok: true }>('PUT', `/api/shelves/${id}/items/order`, { title_ids: titleIds }),
	titleShelves: (kind: Kind, tmdbId: number) =>
		get<{ shelves: { id: number; name: string; on: boolean }[] }>(
			`/api/titles/${kind}/${tmdbId}/shelves`
		),
	pushStatus: () => get<{ public_key: string; devices: number; default_hour: number }>('/api/push'),
	pushSubscribe: (subscription: PushSubscriptionJSON) =>
		send<{ ok: true }>('POST', '/api/push/subscribe', subscription),
	pushUnsubscribe: (endpoint: string) =>
		send<{ ok: true }>('POST', '/api/push/unsubscribe', { endpoint }),
	pushTest: () =>
		send<{ sent: number; gone: number; failed: number }>('POST', '/api/push/test', {}),
	fullStats: (year?: number) => get<FullStats>(`/api/stats${qs({ year })}`),

	title: (kind: Kind, id: number) => get<TitleDetail>(`/api/titles/${kind}/${id}`),
	season: (kind: Kind, id: number, season: number) =>
		get<SeasonDetail>(`/api/titles/${kind}/${id}/seasons/${season}`),
	person: (id: number) => get<PersonDetail>(`/api/person/${id}`),

	track: (
		kind: Kind,
		id: number,
		patch: {
			status?: Status;
			rating?: number | null;
			favourite?: boolean;
			notes?: string | null;
			notify?: boolean;
		}
	) => send<{ item: LibraryItem }>('PUT', `/api/titles/${kind}/${id}/library`, patch),

	untrack: (kind: Kind, id: number) =>
		send<{ ok: true }>('DELETE', `/api/titles/${kind}/${id}/library`),

	setEpisodeWatched: (kind: Kind, id: number, season: number, episode: number, watched: boolean) =>
		send<{ item: LibraryItem }>(
			'POST',
			`/api/titles/${kind}/${id}/episodes/${season}/${episode}/watch`,
			{ watched }
		),

	setSeasonWatched: (kind: Kind, id: number, season: number, watched: boolean, when?: WhenAnswer) =>
		send<{ item: LibraryItem }>('POST', `/api/titles/${kind}/${id}/episodes/${season}/watch`, {
			watched,
			...when
		}),

	setShowWatched: (kind: Kind, id: number, watched: boolean, when?: WhenAnswer) =>
		send<{ item: LibraryItem }>('POST', `/api/titles/${kind}/${id}/episodes/watch`, {
			watched,
			...when,
			extras: undefined,
			specials: when?.extras
		}),

	redateDiary: (kind: Kind, id: number, day: string, when: WhenAnswer) =>
		send<{ changed: number }>('POST', '/api/diary/redate', { kind, tmdb_id: id, day, ...when }),

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
