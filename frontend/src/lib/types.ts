export type Kind = 'movie' | 'tv';

export type Status = 'watching' | 'completed' | 'planned' | 'paused' | 'dropped';

export const STATUSES: Status[] = ['watching', 'completed', 'planned', 'paused', 'dropped'];

export const STATUS_LABEL: Record<Status, string> = {
	watching: 'Watching',
	completed: 'Completed',
	planned: 'Watchlist',
	paused: 'On hold',
	dropped: 'Dropped'
};

export interface Palette {
	accent: string;
	shade: string;
	tint: string;
}

export interface LibraryEntry {
	status: Status;
	rating: number | null;
	favourite: boolean;
	notes: string | null;
	started_at: string | null;
	finished_at: string | null;
	rewatch_count: number;
	added_at: string;
	updated_at: string;
	/** New-episode notifications for this show. */
	notify: boolean;
}

/** A title plus my state: what the grid, the rows and the hero all draw. */
export interface LibraryItem {
	id: number;
	kind: Kind;
	tmdb_id: number;
	name: string;
	overview: string | null;
	poster_path: string | null;
	backdrop_path: string | null;
	logo_path: string | null;
	release_date: string | null;
	last_air_date: string | null;
	tmdb_status: string | null;
	runtime: number | null;
	episode_count: number | null;
	season_count: number | null;
	tmdb_vote: number | null;
	imdb_id: string | null;
	collection_id: number | null;
	genres: string[];
	accent: Palette;
	entry: LibraryEntry | null;
	watched_episodes: number;
	total_episodes: number;
	last_watched_at: string | null;
}

export interface SearchItem {
	kind: Kind;
	tmdb_id: number;
	name: string;
	poster_path: string | null;
	backdrop_path: string | null;
	year: number | null;
	overview: string | null;
	vote: number | null;
	in_library: Status | null;
}

/** One "Because you watched" row on Home. */
export interface BecauseRow {
	seed: { kind: Kind; tmdb_id: number; name: string };
	items: SearchItem[];
}

export interface CastMember {
	id: number;
	name: string;
	character: string | null;
	profile_path: string | null;
	order: number;
}

export interface CrewMember {
	id: number;
	name: string;
	job: string;
	profile_path: string | null;
}

export interface Trailer {
	key: string;
	name: string;
	site: string;
	kind: string;
	official: boolean;
}

export interface Provider {
	id: number;
	name: string;
	logo_path: string | null;
	/** The title's own page on the service, when known. */
	url?: string;
}

export interface ProviderGroup {
	flatrate: Provider[];
	rent: Provider[];
	buy: Provider[];
	link: string | null;
}

export interface SeasonSummary {
	season_number: number;
	name: string;
	episode_count: number;
	poster_path: string | null;
	air_date: string | null;
	overview: string | null;
}

export interface EpisodeView {
	season_number: number;
	episode_number: number;
	tmdb_episode_id: number | null;
	name: string | null;
	overview: string | null;
	still_path: string | null;
	air_date: string | null;
	runtime: number | null;
	tmdb_vote: number | null;
	watched: boolean;
	watch_count: number;
	rating: number | null;
	notes: string | null;
}

export interface SeasonDetail {
	season_number: number;
	name: string | null;
	overview: string | null;
	poster_path: string | null;
	air_date: string | null;
	episodes: EpisodeView[];
}

export interface HeatmapCell {
	s: number;
	e: number;
	rating: number | null;
	/** TMDB's own episode score, rescaled to 0..100. */
	tmdb: number | null;
	watched: boolean;
}

export interface NextUp {
	season_number: number;
	episode_number: number;
	name: string | null;
	still_path: string | null;
	air_date: string | null;
}

export interface Collection {
	id: number;
	name: string | null;
	overview: string | null;
	poster_path: string | null;
	backdrop_path: string | null;
	parts: SearchItem[];
}

export interface TitleDetail {
	item: LibraryItem;
	tagline: string | null;
	homepage: string | null;
	cast: CastMember[];
	crew: CrewMember[];
	trailers: Trailer[];
	providers: ProviderGroup;
	provider_region: string;
	seasons: SeasonSummary[];
	recommendations: SearchItem[];
	similar: SearchItem[];
	collection: Collection | null;
	next_up: NextUp | null;
	heatmap: HeatmapCell[];
	/** Shows only: what can be rated, season by season (or AniList part by part). */
	ratings?: RatingUnit[];
	/** This title has since been found on TMDB and lives there now. */
	moved_to?: { kind: Kind; tmdb_id: number } | null;
	/** Known from AniList alone: no TMDB entry, so no cast, seasons or links. */
	anilist_only?: boolean;
}

/** A season, or a part of one that AniList counts as its own entry. */
export interface RatingUnit {
	season: number;
	first_episode: number;
	episodes: number;
	name: string | null;
	rating: number | null;
	/** Every episode of it is out. */
	rateable: boolean;
}

export interface PersonCredit {
	item: SearchItem;
	role: string | null;
	department: 'cast' | 'crew';
	/** Only ever themselves in it: a talk show, an awards night. */
	appearance?: boolean;
}

export interface PersonDetail {
	id: number;
	name: string | null;
	biography: string | null;
	profile_path: string | null;
	birthday: string | null;
	deathday: string | null;
	place_of_birth: string | null;
	known_for_department: string | null;
	credits: PersonCredit[];
}

export interface AuthStatus {
	needs_setup: boolean;
	authenticated: boolean;
	/** Whoever is signed in, or null. */
	username: string | null;
	/** Address of their profile picture, if they have one. */
	avatar: string | null;
	role: 'owner' | 'admin' | 'user' | null;
	is_admin: boolean;
	tmdb_configured: boolean;
	region: string;
	version: string;
}

export interface Stats {
	by_status: Partial<Record<Status, number>>;
	by_kind: Partial<Record<Kind, number>>;
	films_watched: number;
	episodes_watched: number;
	minutes_watched: number;
	mean_rating: number | null;
}

export type ShelfProgress = 'unstarted' | 'started' | 'unfinished' | 'finished';

/** What a smart shelf matches. Empty matches the whole library. */
export interface ShelfRules {
	kind?: Kind | null;
	statuses?: Status[];
	genres?: string[];
	/** 0-100 */
	min_rating?: number | null;
	max_rating?: number | null;
	progress?: ShelfProgress | null;
	favourite?: boolean;
	from_year?: number | null;
	to_year?: number | null;
	sort?: 'added' | 'updated' | 'name' | 'rating' | 'release' | 'last_watched' | null;
	order?: 'asc' | 'desc' | null;
	limit?: number | null;
}

export interface Shelf {
	id: number;
	name: string;
	kind: 'manual' | 'smart';
	rules: ShelfRules;
	position: number;
	count: number;
	/** Up to four, for the cover. */
	posters: string[];
}

/** A title ranked on the stats page. */
export interface StatsTitle {
	kind: Kind;
	tmdb_id: number;
	name: string;
	poster_path: string | null;
	watches: number;
	minutes: number;
	rating: number | null;
}

export interface StatsPerson {
	tmdb_id: number;
	name: string;
	/** A headshot, or a logo for studios. */
	image: string | null;
	titles: number;
}

export interface Streak {
	days: number;
	from: string | null;
	to: string | null;
}

/** `/api/stats`: everything, or one calendar year when `year` is set. */
export interface FullStats {
	year: number | null;
	years: number[];
	totals: {
		films: number;
		episodes: number;
		shows: number;
		titles: number;
		rewatches: number;
		minutes: number;
		days: number;
		mean_rating: number | null;
	};
	/** Months (YYYY-MM) for a year, years (YYYY) for all time. */
	timeline: { unit: string; minutes: number; films: number; episodes: number }[];
	genres: { name: string; titles: number; minutes: number }[];
	decades: { decade: number; titles: number }[];
	/** Ten buckets of ten points: 0-9, 10-19, ... 90-100. */
	ratings: number[];
	streaks: { longest: Streak; current: Streak };
	top_shows: StatsTitle[];
	top_films: StatsTitle[];
	people: { actors: StatsPerson[]; directors: StatsPerson[]; studios: StatsPerson[] };
	credits_pending: number;
}

export interface NextEpisode {
	season_number: number;
	episode_number: number;
	name: string | null;
	still_path: string | null;
	air_date: string | null;
	aired: boolean;
}

export interface UpNextEntry {
	item: LibraryItem;
	next: NextEpisode | null;
}

export interface AiringEpisode {
	season_number: number;
	episode_number: number;
	name: string | null;
	air_date: string | null;
}

/** One show, with every episode of it due in the window. */
export interface AiringEntry {
	item: LibraryItem;
	episodes: AiringEpisode[];
	air_date: string | null;
}

export interface HomeData {
	hero: UpNextEntry | null;
	/** Part-way through, with an episode already out. */
	continue: UpNextEntry[];
	/** Caught up: the next episode has not aired yet. */
	waiting: UpNextEntry[];
	this_week: AiringEntry[];
	recent: LibraryItem[];
	favourites: LibraryItem[];
}

export interface MatchResult {
	key: string;
	matched: SearchItem | null;
	candidates: SearchItem[];
	note: string | null;
	already_tracked: Status | null;
	season?: number | null;
	last_season?: number | null;
	/** A matched show's seasons on TMDB, specials included. */
	seasons?: { number: number; name: string; episodes: number }[];
	/** The name that found the match, when it was not the row's own title. */
	searched?: string | null;
	/** Which of the row's `episodes` TMDB has by that name, and where. */
	placed?: { index: number; season: number; episode: number }[];
}

export interface ImportSummary {
	added: number;
	updated: number;
	skipped: number;
	failed: { tmdb_id: number; error: string }[];
}

/** An invite link, as the admin page lists them. Never the code itself. */
export interface Invite {
	id: string;
	role: 'admin' | 'user';
	created_by: string | null;
	created_at: string;
	expires_at: string;
	used_at: string | null;
	used_by: string | null;
	state: 'open' | 'used' | 'expired';
}

/** Shown once, when the link is made. */
/** You, as Settings → Account shows it. */
export interface Account {
	id: number;
	username: string;
	role: 'owner' | 'admin' | 'user';
	created_at: string;
	profile_public: boolean;
	two_factor: boolean;
	display_name: string | null;
	bio: string | null;
	/** A TMDB backdrop path used as the banner when no picture is uploaded. */
	banner_backdrop: string | null;
	/** Which part of the banner shows, as "x% y%". */
	banner_position: string | null;
	pins: { kind: Kind; tmdb_id: number }[];
	/** Whether a picture of each kind is uploaded. */
	avatar: boolean;
	banner: boolean;
}

/** Profile fields to change; anything left out stays as it is. */
export interface ProfileEdit {
	public?: boolean;
	display_name?: string;
	bio?: string;
	banner_backdrop?: string;
	banner_position?: string;
	pins?: { kind: Kind; tmdb_id: number }[];
}

export interface PersonSummary {
	username: string;
	display_name: string | null;
	me: boolean;
	avatar: boolean;
}

/** One device signed in to your account. */
export interface DeviceSession {
	id: string;
	created_at: string;
	expires_at: string;
	device: string | null;
	current: boolean;
}

export interface AdminUser {
	id: number;
	username: string;
	role: 'owner' | 'admin' | 'user';
	disabled: boolean;
	created_at: string;
	titles: number;
	last_sign_in: string | null;
}

export interface InstanceInfo {
	version: string;
	database_bytes: number;
	image_cache_bytes: number;
	accounts: number;
	last_backup: { at: string; label: string; pushed: boolean } | null;
}

/** The AniList connection as Settings shows it. */
export interface AniListStatus {
	/** False when this Frog has no AniList client set up. */
	available: boolean;
	connected?: boolean;
	name?: string;
	expires_at?: string;
	synced_at?: string | null;
	syncing?: boolean;
	error?: string | null;
	summary?: {
		to_frog: number;
		to_anilist: number;
		unsure: number;
		entries: number;
		upcoming?: number;
		repaired?: number;
		/** In the library from AniList alone, with no TMDB title yet. */
		local?: number;
		/** Could not be written this time; tried again next sync. */
		failed?: number;
		/** No longer on the AniList list; kept in Frog. */
		removed?: number;
	} | null;
	unsure?: number;
	/** The last attempt, successful or not. synced_at is the last one that finished. */
	attempted_at?: string | null;
	next_sync_at?: string | null;
	/** AniList refused the token: only connecting again helps. */
	needs_reconnect?: boolean;
	failures?: number;
	interval_minutes?: number;
}

export interface AniListCandidate {
	kind: Kind;
	tmdb_id: number;
	name: string;
	year: string | null;
	poster_path: string | null;
	/** What keeps it from being a sure match. */
	problems?: string[];
}

export interface AniListEntry {
	media_id: number;
	name: string;
	format: string | null;
	episodes: number | null;
	state: 'placed' | 'unsure' | 'ignored';
	candidates: AniListCandidate[];
	outcome: 'matched' | 'not_confident' | 'no_candidate' | 'upcoming' | 'apply_failed';
	/** How it was placed: an id list, the air date, the season before it, your choice… */
	method: string | null;
	confidence: number | null;
	/** Why it is where it is, or why it is not placed. */
	note: string | null;
	manual: boolean;
	/** AniList's release state: FINISHED, RELEASING, NOT_YET_RELEASED, HIATUS, CANCELLED. */
	media_status: string | null;
	start_date: string | null;
	title: {
		name: string;
		poster_path: string | null;
		kind: Kind;
		tmdb_id: number;
		season: number | null;
		from_episode: number;
	} | null;
	status: string | null;
	progress: number | null;
	/** No longer on the AniList list; kept in Frog as it is. */
	gone?: boolean;
	/** When Frog next looks for its place. */
	next_try_at?: string | null;
	/** In the library as it is on AniList until it is placed. */
	kept?: { kind: Kind; tmdb_id: number } | null;
}

export type AniListChoice =
	{ ignore: true } | { kind: Kind; tmdb_id: number; season?: number; from_episode?: number };

export interface ProfileCard {
	kind: Kind;
	tmdb_id: number;
	name: string;
	poster_path: string | null;
	rating: number | null;
}

export interface Profile {
	username: string;
	display_name: string | null;
	bio: string | null;
	joined: string;
	public: boolean;
	me: boolean;
	avatar: string | null;
	banner: string | null;
	banner_backdrop: string | null;
	banner_position: string | null;
	stats: {
		films: number;
		episodes: number;
		shows: number;
		minutes: number;
		library: number;
		completed: number;
		favourites: number;
		mean_rating: number | null;
		streak: number;
		longest_streak: number;
	};
	genres: { name: string; titles: number }[];
	pinned: ProfileCard[];
	watching: ProfileCard[];
	favourites: ProfileCard[];
	recent: ProfileCard[];
}

export interface NewInvite {
	code: string;
	path: string;
	role: 'admin' | 'user';
	expires_at: string;
}

/** Someone on TMDB, from a search. */
export interface PersonResult {
	id: number;
	name: string;
	profile_path: string | null;
	department: string | null;
	known_for: string[];
}

export interface StudioResult {
	id: number;
	name: string;
	logo_path: string | null;
	country: string | null;
}

/** A character: from a cast list (TMDB), or from AniList for anime. */
export interface CharacterResult {
	name: string;
	image: { source: 'tmdb' | 'anilist'; path: string } | null;
	actor: { id: number; name: string } | null;
	title: {
		kind: Kind | null;
		tmdb_id: number | null;
		name: string | null;
		poster_path: string | null;
	};
}

export interface MoreResults {
	people: PersonResult[];
	studios: StudioResult[];
	characters: CharacterResult[];
}

export interface StudioDetail {
	id: number;
	name: string | null;
	logo_path: string | null;
	description: string | null;
	headquarters: string | null;
	country: string | null;
	homepage: string | null;
	titles: SearchItem[];
}
