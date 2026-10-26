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
}

export interface PersonCredit {
	item: SearchItem;
	role: string | null;
	department: 'cast' | 'crew';
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
	username: string | null;
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
}

export interface ImportSummary {
	added: number;
	updated: number;
	skipped: number;
	failed: { tmdb_id: number; error: string }[];
}
