//! AniList, both ways.
//!
//! Each account connects its own AniList through AniList's sign-in; the token
//! stays on the server. A sync reads the whole AniList list, places every
//! entry in Frog by id (never by name), and then, entry by entry, carries
//! across whichever side changed since the last sync. Episodes ticked, a
//! status changed or a score given in Frog go to AniList; the same done on
//! AniList comes to Frog.
//!
//! Placing is the hard part. AniList has one entry per season or cour; TMDB
//! often has one long season instead (Jujutsu Kaisen's second season is TMDB
//! season 1 from episode 25). The community Anime-IDs list says which TVDB
//! season and offset each AniList entry is; from that and the episode counts
//! of the seasons before it, the entry's absolute position is known, and TMDB's
//! own season lengths turn that into a TMDB season and episode. Anything that
//! cannot be placed for certain is left for the person to choose.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;

pub mod media;
mod place;

pub use place::{
    clean, locate, main_run, place_tv, plain, Outcome, Placement, Target, CONFIDENT, RESOLVER,
};

use crate::config::AniListClient;
use crate::error::{AppError, AppResult};
use crate::release;
use crate::routes::transfer::merged_status;
use crate::state::AppState;
use crate::store::{self, LibraryPatch};

/// AniList allows 30 requests a minute at the moment; stay under it.
const SPACING: Duration = Duration::from_millis(2_100);

/// How often the worker re-reads a list nobody touched, in minutes: one
/// request per 500 entries, so a change made on AniList (an episode ticked
/// by a streaming site, say) shows up in Frog within minutes without Frog
/// being open. `ANILIST_SYNC_MINUTES` sets it, between 1 and 60.
pub fn sync_minutes() -> i64 {
    static MINUTES: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *MINUTES.get_or_init(|| {
        std::env::var("ANILIST_SYNC_MINUTES")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .unwrap_or(3)
            .clamp(1, 60)
    })
}

/// Tries of one request before it counts as failed.
const ATTEMPTS: u32 = 4;

/// The longest a failing account waits before it is tried again.
const MAX_BACKOFF_MINUTES: i64 = 60;

/// AniList hands out a list 500 entries at a time at most.
const PER_CHUNK: i64 = 500;

/// Lists longer than this many chunks are not read; nobody has 20,000.
const MAX_CHUNKS: i64 = 40;

const SIGNED_OUT: &str = "AniList signed Frog out. Connect it again in Settings.";

/// Both id lists change slowly; a week is plenty.
const MAP_DAYS: i64 = 7;

/// TMDB's genre id for Animation.
const ANIMATION: i64 = 16;

static GATE: tokio::sync::Mutex<Option<Instant>> = tokio::sync::Mutex::const_new(None);

/// Accounts with a sync running, so two never overlap.
static RUNNING: std::sync::Mutex<Option<HashSet<i64>>> = std::sync::Mutex::new(None);

pub fn is_syncing(user_id: i64) -> bool {
    RUNNING
        .lock()
        .map(|g| g.as_ref().is_some_and(|s| s.contains(&user_id)))
        .unwrap_or(false)
}

struct RunningGuard(i64);

impl RunningGuard {
    fn claim(user_id: i64) -> Option<Self> {
        let mut guard = RUNNING.lock().ok()?;
        let set = guard.get_or_insert_with(HashSet::new);
        // Built only once the lock is let go: dropping a guard takes the lock.
        if !set.insert(user_id) {
            return None;
        }
        drop(guard);
        Some(Self(user_id))
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        if let Ok(mut guard) = RUNNING.lock() {
            if let Some(set) = guard.as_mut() {
                set.remove(&self.0);
            }
        }
    }
}

/// Something changed in Frog: the next worker pass pushes it.
pub async fn touch(pool: &SqlitePool, user_id: i64) {
    let _ = sqlx::query("UPDATE anilist_link SET dirty = 1 WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await;
}

// --- talking to AniList ------------------------------------------------------

pub struct Api<'a> {
    pub http: &'a reqwest::Client,
    pub client: &'a AniListClient,
}

impl<'a> Api<'a> {
    pub fn new(state: &'a AppState) -> AppResult<Self> {
        let client = state
            .config
            .anilist
            .as_ref()
            .ok_or_else(|| AppError::NotFound("AniList is not set up on this Frog.".into()))?;
        Ok(Self {
            http: &state.http,
            client,
        })
    }

    fn is_live(&self) -> bool {
        self.client
            .api_url
            .starts_with("https://graphql.anilist.co")
    }

    pub fn authorize_url(&self, state: &str) -> String {
        format!(
            "{}/api/v2/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&state={}",
            self.client.site_url,
            urlencoding::encode(&self.client.id),
            urlencoding::encode(&self.client.redirect_uri),
            urlencoding::encode(state),
        )
    }

    /// Trade the code from the sign-in redirect for a token and its lifetime.
    pub async fn exchange(&self, code: &str) -> AppResult<(String, i64)> {
        let resp = self
            .http
            .post(format!("{}/api/v2/oauth/token", self.client.site_url))
            .header("accept", "application/json")
            .json(&json!({
                "grant_type": "authorization_code",
                "client_id": self.client.id,
                "client_secret": self.client.secret,
                "redirect_uri": self.client.redirect_uri,
                "code": code,
            }))
            .send()
            .await
            .map_err(|e| AppError::AniList(e.to_string()))?;
        let status = resp.status();
        let body: Value = resp.json().await.unwrap_or(Value::Null);
        let token = body["access_token"].as_str().map(str::to_string);
        match token {
            Some(t) if status.is_success() => {
                // AniList says a year; trust what it answers when it answers.
                let seconds = body["expires_in"].as_i64().unwrap_or(365 * 24 * 3600);
                Ok((t, seconds))
            }
            _ => Err(AppError::BadRequest(
                "AniList did not accept that sign-in. Try connecting again.".into(),
            )),
        }
    }

    /// Wait before another try: doubling from a couple of seconds, with
    /// jitter so two accounts failing together do not retry in step. A
    /// stand-in AniList in tests gets milliseconds.
    async fn backoff(&self, attempt: u32) {
        let base = if self.is_live() { 2_000.0 } else { 20.0 };
        let jitter: f64 = {
            use rand::Rng;
            rand::thread_rng().gen_range(0.5..1.5)
        };
        let ms = base * f64::from(1u32 << attempt.min(5)) * jitter;
        tokio::time::sleep(Duration::from_millis(ms as u64)).await;
    }

    /// One GraphQL call, paced under AniList's limit. A rate limit waits as
    /// long as AniList asks; a dropped connection, a server error or an
    /// unreadable answer is tried again with backoff, a few times at most. A
    /// refused token is not retried: it needs the person to connect again.
    pub async fn graphql(&self, token: &str, query: &str, variables: Value) -> AppResult<Value> {
        let mut last_error = String::from("no answer");
        for attempt in 0..ATTEMPTS {
            {
                let mut last = GATE.lock().await;
                if self.is_live() {
                    if let Some(at) = *last {
                        let wait = SPACING.saturating_sub(at.elapsed());
                        if !wait.is_zero() {
                            tokio::time::sleep(wait).await;
                        }
                    }
                }
                *last = Some(Instant::now());
            }

            let mut req = self.http.post(&self.client.api_url);
            // No token is a public read, such as a search.
            if !token.is_empty() {
                req = req.bearer_auth(token);
            }
            let resp = match req
                .header("accept", "application/json")
                .json(&json!({ "query": query, "variables": variables }))
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    last_error = if e.is_timeout() {
                        "timed out".into()
                    } else {
                        "could not connect".into()
                    };
                    self.backoff(attempt).await;
                    continue;
                }
            };

            let status = resp.status();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let secs = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(60)
                    .min(65);
                tracing::info!(secs, "AniList asked Frog to slow down");
                last_error = "rate limited".into();
                if attempt + 1 < ATTEMPTS {
                    tokio::time::sleep(Duration::from_secs(secs)).await;
                }
                continue;
            }
            if status == reqwest::StatusCode::UNAUTHORIZED {
                return Err(AppError::Forbidden(SIGNED_OUT.into()));
            }
            if status.is_server_error() {
                last_error = status.to_string();
                self.backoff(attempt).await;
                continue;
            }
            let body: Value = match resp.json().await {
                Ok(body) => body,
                Err(_) => {
                    // Half an answer is no answer: never read as an empty list.
                    last_error = format!("{status}: unreadable answer");
                    self.backoff(attempt).await;
                    continue;
                }
            };
            if let Some(err) = body["errors"].as_array().and_then(|a| a.first()) {
                let message = err["message"].as_str().unwrap_or("unknown error");
                if message.to_lowercase().contains("invalid token") {
                    return Err(AppError::Forbidden(SIGNED_OUT.into()));
                }
                return Err(AppError::AniList(format!("{status}: {message}")));
            }
            if !status.is_success() {
                return Err(AppError::AniList(status.to_string()));
            }
            if body["data"].is_null() {
                last_error = format!("{status}: no data");
                self.backoff(attempt).await;
                continue;
            }
            return Ok(body["data"].clone());
        }
        Err(AppError::AniList(last_error))
    }

    pub async fn viewer(&self, token: &str) -> AppResult<(i64, String)> {
        let data = self
            .graphql(token, "query { Viewer { id name } }", json!({}))
            .await?;
        let id = data["Viewer"]["id"]
            .as_i64()
            .ok_or_else(|| AppError::AniList("no viewer".into()))?;
        let name = data["Viewer"]["name"].as_str().unwrap_or("").to_string();
        Ok((id, name))
    }
}

const LIST_QUERY: &str = "query ($userId: Int, $chunk: Int, $perChunk: Int) {
  MediaListCollection(userId: $userId, type: ANIME, chunk: $chunk, perChunk: $perChunk) {
    hasNextChunk
    lists { entries {
      status
      score(format: POINT_100)
      progress
      media {
        id format status episodes seasonYear countryOfOrigin synonyms
        startDate { year month day }
        nextAiringEpisode { episode airingAt }
        title { english romaji native }
      }
    } }
  }
}";

const MEDIA_QUERY: &str = "query ($ids: [Int]) {
  Page(perPage: 50) { media(id_in: $ids, type: ANIME) { id format episodes title { english romaji } } }
}";

const SAVE_MUTATION: &str =
    "mutation ($mediaId: Int, $status: MediaListStatus, $progress: Int, $scoreRaw: Int) {
  SaveMediaListEntry(mediaId: $mediaId, status: $status, progress: $progress, scoreRaw: $scoreRaw) {
    status progress score(format: POINT_100)
  }
}";

/// One entry as AniList has it.
#[derive(Debug, Clone)]
pub struct Remote {
    pub media_id: i64,
    pub name: String,
    /// Every title AniList gives it (English, romaji, native), for matching
    /// by name when no list knows the id.
    pub titles: Vec<String>,
    /// Other names it goes by, for comparing against TMDB's.
    pub synonyms: Vec<String>,
    pub format: Option<String>,
    pub episodes: Option<i64>,
    pub year: Option<i64>,
    /// yyyy-mm-dd, when AniList knows the whole date.
    pub start: Option<String>,
    /// FINISHED, RELEASING, NOT_YET_RELEASED, CANCELLED or HIATUS.
    pub media_status: Option<String>,
    pub country: Option<String>,
    /// (episode, unix time) of the next episode to air.
    pub next_airing: Option<(i64, i64)>,
    pub view: View,
}

/// The three things that sync: list status (in AniList's words), how many
/// episodes in, and a 0–100 score where 0 is none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct View {
    pub status: String,
    pub progress: i64,
    pub score: i64,
}

fn media_name(media: &Value) -> String {
    media["title"]["english"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| media["title"]["romaji"].as_str())
        .unwrap_or("Untitled")
        .to_string()
}

pub fn parse_list(data: &Value) -> Vec<Remote> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let lists = data["MediaListCollection"]["lists"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for list in lists {
        for e in list["entries"].as_array().cloned().unwrap_or_default() {
            let media = &e["media"];
            let Some(media_id) = media["id"].as_i64() else {
                continue;
            };
            // Custom lists repeat entries that are already in a status list.
            if !seen.insert(media_id) {
                continue;
            }
            let status = e["status"].as_str().unwrap_or("PLANNING");
            out.push(Remote {
                media_id,
                name: media_name(media),
                titles: place::titles_of(media),
                synonyms: media["synonyms"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect(),
                format: media["format"].as_str().map(str::to_string),
                episodes: media["episodes"].as_i64().filter(|n| *n > 0),
                // The year it started, which for a film is the year it came
                // out; seasonYear is missing for most films.
                year: media["startDate"]["year"]
                    .as_i64()
                    .or_else(|| media["seasonYear"].as_i64()),
                start: place::full_date(&media["startDate"]),
                media_status: media["status"].as_str().map(str::to_string),
                country: media["countryOfOrigin"].as_str().map(str::to_string),
                next_airing: media["nextAiringEpisode"]["episode"]
                    .as_i64()
                    .zip(media["nextAiringEpisode"]["airingAt"].as_i64()),
                view: View {
                    // A rewatch is still watching, as far as Frog is concerned.
                    status: if status == "REPEATING" {
                        "CURRENT".into()
                    } else {
                        status.to_string()
                    },
                    progress: e["progress"].as_i64().unwrap_or(0).max(0),
                    score: e["score"].as_f64().unwrap_or(0.0).round() as i64,
                },
            });
        }
    }
    out
}

/// A whole list, chunk by chunk. Any chunk that fails fails the lot: a list
/// read halfway would look like entries removed.
async fn fetch_list(api: &Api<'_>, token: &str, query: &str, who: Value) -> AppResult<Vec<Remote>> {
    let mut lists: Vec<Value> = Vec::new();
    for chunk in 1..=MAX_CHUNKS {
        let mut vars = who.clone();
        vars["chunk"] = json!(chunk);
        vars["perChunk"] = json!(PER_CHUNK);
        let data = api.graphql(token, query, vars).await?;
        let collection = &data["MediaListCollection"];
        if !collection["lists"].is_array() {
            return Err(AppError::AniList(
                "the list came back without entries".into(),
            ));
        }
        lists.extend(collection["lists"].as_array().cloned().unwrap_or_default());
        if collection["hasNextChunk"].as_bool() != Some(true) {
            return Ok(parse_list(
                &json!({ "MediaListCollection": { "lists": lists } }),
            ));
        }
    }
    Err(AppError::AniList(
        "the list is longer than Frog reads".into(),
    ))
}

// --- which TMDB / TVDB / IMDb entry an AniList id is -------------------------

/// One entry of Fribb's anime-lists; everything else in it is left unread.
#[derive(Deserialize)]
struct ListEntry {
    anilist_id: Option<i64>,
    /// `{ "tv": 1429 }` or `{ "movie": 129 }`.
    themoviedb_id: Option<Value>,
    /// `{ "tvdb": 2, "tmdb": 1 }`.
    season: Option<Value>,
    /// `{ "tvdb": 0, "tmdb": 28 }`.
    episode_offset: Option<Value>,
    #[serde(rename = "type")]
    format: Option<String>,
}

async fn download<T: serde::de::DeserializeOwned>(state: &AppState, url: &str) -> Option<T> {
    match state.http.get(url).send().await {
        Ok(r) if r.status().is_success() => match r.json::<T>().await {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!(%url, error = %e, "an anime id list was not the expected JSON");
                None
            }
        },
        Ok(r) => {
            tracing::warn!(%url, status = %r.status(), "could not download an anime id list");
            None
        }
        Err(e) => {
            tracing::warn!(%url, error = %e, "could not download an anime id list");
            None
        }
    }
}

/// Make sure both id lists are loaded and not older than a week: Anime-IDs
/// (TVDB and IMDb ids) and Fribb's anime-lists (TMDB ids and TMDB seasons).
/// Each list replaces only its own columns, so one failed download keeps
/// whatever that list said last time.
pub async fn ensure_map(state: &AppState, client: &AniListClient) -> AppResult<()> {
    let row: Option<(String,)> = sqlx::query_as("SELECT fetched_at FROM anime_map_state")
        .fetch_optional(&state.pool)
        .await?;
    let fresh = row
        .and_then(|(at,)| chrono::DateTime::parse_from_rfc3339(&at).ok())
        .is_some_and(|at| Utc::now() - at.with_timezone(&Utc) < chrono::Duration::days(MAP_DAYS));
    if fresh {
        return Ok(());
    }

    let ids: Option<serde_json::Map<String, Value>> = download(state, &client.anime_ids_url).await;
    let lists: Option<Vec<ListEntry>> = download(state, &client.anime_lists_url).await;
    if ids.is_none() && lists.is_none() {
        return Ok(());
    }

    let mut tx = state.pool.begin().await?;
    let mut from_ids = 0;
    if let Some(entries) = &ids {
        sqlx::query(
            "UPDATE anime_map
                SET tvdb_id = NULL, tvdb_season = NULL, tvdb_epoffset = 0, imdb_id = NULL",
        )
        .execute(&mut *tx)
        .await?;
        for v in entries.values() {
            let Some(anilist_id) = v["anilist_id"].as_i64() else {
                continue;
            };
            let imdb = match &v["imdb_id"] {
                Value::String(s) => s.split(',').next().map(|s| s.trim().to_string()),
                _ => None,
            }
            .filter(|s| s.starts_with("tt"));
            sqlx::query(
                "INSERT INTO anime_map (anilist_id, tvdb_id, tvdb_season, tvdb_epoffset, imdb_id)
                 VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT (anilist_id) DO UPDATE SET
                    tvdb_id = excluded.tvdb_id, tvdb_season = excluded.tvdb_season,
                    tvdb_epoffset = excluded.tvdb_epoffset, imdb_id = excluded.imdb_id",
            )
            .bind(anilist_id)
            .bind(v["tvdb_id"].as_i64())
            .bind(v["tvdb_season"].as_i64())
            .bind(v["tvdb_epoffset"].as_i64().unwrap_or(0))
            .bind(imdb)
            .execute(&mut *tx)
            .await?;
            from_ids += 1;
        }
    }
    let mut from_lists = 0;
    if let Some(entries) = &lists {
        sqlx::query(
            "UPDATE anime_map
                SET tmdb_kind = NULL, tmdb_id = NULL, tmdb_season = NULL, tmdb_offset = NULL",
        )
        .execute(&mut *tx)
        .await?;
        for e in entries {
            let (Some(anilist_id), Some(tmdb)) = (e.anilist_id, e.themoviedb_id.as_ref()) else {
                continue;
            };
            let film = e.format.as_deref() == Some("MOVIE");
            let (kind, tmdb_id) = match (tmdb["movie"].as_i64(), tmdb["tv"].as_i64()) {
                (Some(m), _) if film => ("movie", m),
                (_, Some(t)) => ("tv", t),
                (Some(m), None) => ("movie", m),
                (None, None) => continue,
            };
            let season = e.season.as_ref().and_then(|s| s["tmdb"].as_i64());
            let offset = e.episode_offset.as_ref().and_then(|o| o["tmdb"].as_i64());
            sqlx::query(
                "INSERT INTO anime_map (anilist_id, tmdb_kind, tmdb_id, tmdb_season, tmdb_offset)
                 VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT (anilist_id) DO UPDATE SET
                    tmdb_kind = excluded.tmdb_kind, tmdb_id = excluded.tmdb_id,
                    tmdb_season = excluded.tmdb_season, tmdb_offset = excluded.tmdb_offset",
            )
            .bind(anilist_id)
            .bind(kind)
            .bind(tmdb_id)
            .bind(season)
            .bind(offset)
            .execute(&mut *tx)
            .await?;
            from_lists += 1;
        }
    }
    sqlx::query(
        "INSERT INTO anime_map_state (id, fetched_at) VALUES (1, ?)
         ON CONFLICT (id) DO UPDATE SET fetched_at = excluded.fetched_at",
    )
    .bind(Utc::now().to_rfc3339())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    tracing::info!(from_ids, from_lists, "loaded the anime id lists");
    Ok(())
}

async fn remember_media(
    pool: &SqlitePool,
    id: i64,
    episodes: Option<i64>,
    format: Option<&str>,
    name: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO anilist_media (id, episodes, format, name, fetched_at) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET episodes = excluded.episodes, format = excluded.format,
                                        name = excluded.name, fetched_at = excluded.fetched_at",
    )
    .bind(id)
    .bind(episodes)
    .bind(format)
    .bind(name)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

// --- what Frog says about an entry -------------------------------------------

/// The episodes an entry covers, in order, with whether each has come out:
/// from `offset` into `season`, carrying on into later seasons. An entry
/// whose length AniList does not know (a long series still airing) runs on
/// until the next AniList entry of the same show, or to the end of the show,
/// never just to the end of a TMDB season. Specials stay within season 0.
async fn run_ids(
    pool: &SqlitePool,
    user_id: i64,
    media_id: i64,
    title_id: i64,
    season: i64,
    offset: i64,
    episodes: Option<i64>,
) -> AppResult<Vec<(i64, bool)>> {
    if season == 0 {
        let sql = format!(
            "SELECT e.id, {} FROM episode e
              WHERE e.title_id = ? AND e.season_number = 0 AND e.episode_number > ?
              ORDER BY e.episode_number
              LIMIT ?",
            release::RELEASED
        );
        return Ok(sqlx::query_as(&sql)
            .bind(title_id)
            .bind(offset)
            .bind(episodes.unwrap_or(1).max(1))
            .fetch_all(pool)
            .await?);
    }
    // (id, season, episode, released)
    let sql = format!(
        "SELECT e.id, e.season_number, e.episode_number, {} FROM episode e
          WHERE e.title_id = ? AND e.season_number > 0
          ORDER BY e.season_number, e.episode_number",
        release::RELEASED
    );
    let rows: Vec<(i64, i64, i64, bool)> =
        sqlx::query_as(&sql).bind(title_id).fetch_all(pool).await?;
    let others: Vec<(i64, i64, Option<i64>)> = sqlx::query_as(
        "SELECT season, episode_offset, episodes FROM anilist_entry
          WHERE user_id = ? AND title_id = ? AND state = 'placed' AND season > 0
            AND media_id != ?",
    )
    .bind(user_id)
    .bind(title_id)
    .bind(media_id)
    .fetch_all(pool)
    .await?;
    let order: Vec<(i64, i64)> = rows.iter().map(|r| (r.1, r.2)).collect();
    let mut spans = vec![(season, offset, episodes)];
    spans.extend(others);
    let (start, end) = release::ranges(&order, &spans)[0];
    Ok(rows[start..end].iter().map(|r| (r.0, r.3)).collect())
}

async fn watched_among(pool: &SqlitePool, user_id: i64, ids: &[i64]) -> AppResult<i64> {
    let mut n = 0;
    for chunk in ids.chunks(200) {
        let marks = vec!["?"; chunk.len()].join(",");
        let sql = format!(
            "SELECT COUNT(DISTINCT episode_id) FROM watch_event
              WHERE user_id = ? AND episode_id IN ({marks})"
        );
        let mut q = sqlx::query_as::<_, (i64,)>(&sql).bind(user_id);
        for id in chunk {
            q = q.bind(id);
        }
        n += q.fetch_one(pool).await?.0;
    }
    Ok(n)
}

fn frog_to_anilist(status: &str) -> &'static str {
    match status {
        "watching" => "CURRENT",
        "completed" => "COMPLETED",
        "paused" => "PAUSED",
        "dropped" => "DROPPED",
        _ => "PLANNING",
    }
}

fn anilist_to_frog(status: &str) -> &'static str {
    match status {
        "CURRENT" | "REPEATING" => "watching",
        "COMPLETED" => "completed",
        "PAUSED" => "paused",
        "DROPPED" => "dropped",
        _ => "planned",
    }
}

/// The rating Frog keeps for the season (or part of one) an entry covers.
async fn part_rating(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    season: i64,
    offset: i64,
) -> AppResult<Option<f64>> {
    let row: Option<(f64,)> = sqlx::query_as(
        "SELECT rating FROM season_rating
          WHERE user_id = ? AND title_id = ? AND season_number = ? AND first_episode = ?",
    )
    .bind(user_id)
    .bind(title_id)
    .bind(season)
    .bind(offset + 1)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}

/// Rate (or with `None`, unrate) the season or part an entry covers.
pub async fn set_part_rating(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    season: i64,
    first_episode: i64,
    rating: Option<f64>,
) -> AppResult<()> {
    match rating {
        Some(r) => {
            sqlx::query(
                "INSERT INTO season_rating
                        (user_id, title_id, season_number, first_episode, rating, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?)
                 ON CONFLICT (user_id, title_id, season_number, first_episode)
                 DO UPDATE SET rating = excluded.rating, updated_at = excluded.updated_at",
            )
            .bind(user_id)
            .bind(title_id)
            .bind(season)
            .bind(first_episode)
            .bind(r.clamp(0.0, 100.0))
            .bind(Utc::now().to_rfc3339())
            .execute(pool)
            .await?;
        }
        None => {
            sqlx::query(
                "DELETE FROM season_rating
                  WHERE user_id = ? AND title_id = ? AND season_number = ? AND first_episode = ?",
            )
            .bind(user_id)
            .bind(title_id)
            .bind(season)
            .bind(first_episode)
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

/// An entry as Frog sees it, or `None` when the title is not in the library.
async fn frog_view(
    pool: &SqlitePool,
    user_id: i64,
    media_id: i64,
    title_id: i64,
    target: Target,
    episodes: Option<i64>,
) -> AppResult<Option<View>> {
    let Some(entry) = store::get_entry(pool, user_id, title_id).await? else {
        return Ok(None);
    };
    let view = match target {
        Target::Movie(_) => {
            let (watches,): (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM watch_event
                  WHERE user_id = ? AND title_id = ? AND episode_id IS NULL",
            )
            .bind(user_id)
            .bind(title_id)
            .fetch_one(pool)
            .await?;
            let done = entry.status == "completed" || watches > 0;
            View {
                status: if done {
                    "COMPLETED".into()
                } else {
                    frog_to_anilist(&entry.status).into()
                },
                progress: i64::from(done),
                score: entry.rating.map(|r| r.round() as i64).unwrap_or(0),
            }
        }
        Target::Show { season, offset, .. } => {
            let run = run_ids(pool, user_id, media_id, title_id, season, offset, episodes).await?;
            let ids: Vec<i64> = run.iter().map(|r| r.0).collect();
            let progress = watched_among(pool, user_id, &ids).await?;
            let finished = episodes.is_some_and(|n| n > 0 && progress >= n);
            let status = match entry.status.as_str() {
                "paused" | "dropped" if !finished => frog_to_anilist(&entry.status),
                _ if finished => "COMPLETED",
                _ if progress > 0 => "CURRENT",
                _ => "PLANNING",
            };
            // Each AniList entry is scored on its own, like each season here.
            let score = part_rating(pool, user_id, title_id, season, offset)
                .await?
                .map(|r| r.round() as i64)
                .unwrap_or(0);
            View {
                status: status.into(),
                progress,
                score,
            }
        }
    };
    Ok(Some(view))
}

/// Before a person moves an entry somewhere else, take back what its old
/// place was given from AniList; the next sync lays it out afresh.
pub async fn forget_place(state: &AppState, user_id: i64, media_id: i64) -> AppResult<()> {
    type Row = (
        Option<i64>,
        Option<i64>,
        i64,
        Option<i64>,
        Option<String>,
        Option<i64>,
    );
    let row: Option<Row> = sqlx::query_as(
        "SELECT a.title_id, a.season, a.episode_offset, a.episodes, t.kind, t.tmdb_id
           FROM anilist_entry a LEFT JOIN title t ON t.id = a.title_id
          WHERE a.user_id = ? AND a.media_id = ? AND a.state = 'placed'",
    )
    .bind(user_id)
    .bind(media_id)
    .fetch_optional(&state.pool)
    .await?;
    let Some((Some(title_id), season, offset, episodes, Some(kind), Some(tmdb_id))) = row else {
        return Ok(());
    };
    let old = match (kind.as_str(), season) {
        ("movie", _) => Target::Movie(tmdb_id),
        (_, Some(season)) => Target::Show {
            tmdb_id,
            season,
            offset,
        },
        _ => return Ok(()),
    };
    unplace(state, user_id, (media_id, episodes), title_id, old, None).await
}

// --- bringing AniList's side into Frog ---------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// First sync of an entry: only add and move forward.
    Merge,
    /// AniList changed since the last sync: it is the truth.
    Follow,
}

async fn ensure_episodes(state: &AppState, title_id: i64, detail: &Value) -> AppResult<()> {
    let (have,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM episode WHERE title_id = ?")
        .bind(title_id)
        .fetch_one(&state.pool)
        .await?;
    if have == 0 {
        store::sync_all_seasons(state, title_id, detail).await?;
    }
    Ok(())
}

/// Make Frog match AniList for one entry. Says whether the title was new to
/// the library, in which case its status is AniList's outright.
async fn apply(
    state: &AppState,
    user_id: i64,
    remote: &Remote,
    target: Target,
    mode: Mode,
    before: Option<&View>,
) -> AppResult<bool> {
    let (kind, tmdb_id) = match target {
        Target::Movie(id) => ("movie", id),
        Target::Show { tmdb_id, .. } => ("tv", tmdb_id),
    };
    let (title_id, detail) = store::ensure_title(state, kind, tmdb_id).await?;
    let existing = store::get_entry(&state.pool, user_id, title_id).await?;
    let a = &remote.view;

    // The score, if it should change: Some(new), where new may be None.
    let rating: Option<Option<f64>> = {
        let current = match target {
            Target::Movie(_) => existing.as_ref().and_then(|e| e.rating),
            Target::Show { season, offset, .. } => {
                part_rating(&state.pool, user_id, title_id, season, offset).await?
            }
        };
        if a.score > 0 {
            match mode {
                Mode::Merge => current.is_none().then_some(Some(a.score as f64)),
                Mode::Follow => Some(Some(a.score as f64)),
            }
        } else if mode == Mode::Follow && before.is_some_and(|b| b.score > 0) {
            // The score was taken off on AniList.
            Some(None)
        } else {
            None
        }
    };

    let incoming = anilist_to_frog(&a.status).to_string();
    let status = match (&existing, kind) {
        // A show's status follows its seasons together; see `settle_show`.
        (None, "tv") => Some("planned".to_string()),
        (Some(_), "tv") => None,
        (None, _) => Some(incoming),
        (Some(e), _) if mode == Mode::Merge => Some(merged_status(&e.status, &incoming)),
        (Some(_), _) => Some(incoming),
    };
    let patch = LibraryPatch {
        status,
        rating: match target {
            Target::Movie(_) => rating,
            Target::Show { .. } => None,
        },
        ..Default::default()
    };
    store::apply_patch(&state.pool, user_id, title_id, &patch).await?;

    match target {
        Target::Movie(_) => {
            if a.status == "COMPLETED" {
                film_watch(&state.pool, user_id, title_id).await?;
            }
        }
        Target::Show { season, offset, .. } => {
            if let Some(r) = rating {
                set_part_rating(&state.pool, user_id, title_id, season, offset + 1, r).await?;
            }
            ensure_episodes(state, title_id, &detail).await?;
            let run = run_ids(
                &state.pool,
                user_id,
                remote.media_id,
                title_id,
                season,
                offset,
                remote.episodes,
            )
            .await?;
            let upto = a.progress.max(0) as usize;
            // Only what has come out: progress never ticks an episode that
            // has not aired.
            let tick: Vec<i64> = run
                .iter()
                .take(upto)
                .filter(|(_, released)| *released)
                .map(|(id, _)| *id)
                .collect();
            if upto > run.len() {
                tracing::info!(
                    media_id = remote.media_id,
                    progress = upto,
                    tmdb = run.len(),
                    "AniList progress is past the episodes TMDB lists"
                );
            }
            for id in &tick {
                sqlx::query(
                    "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch,
                                              source, undated)
                     SELECT ?, ?, ?, ?, 0, 'anilist', 1
                      WHERE NOT EXISTS (SELECT 1 FROM watch_event WHERE user_id = ? AND episode_id = ?)",
                )
                .bind(user_id)
                .bind(title_id)
                .bind(id)
                .bind(Utc::now().to_rfc3339())
                .bind(user_id)
                .bind(id)
                .execute(&state.pool)
                .await?;
            }
            if mode == Mode::Follow {
                // Progress went down on AniList: take back the ticks AniList
                // gave. A watch logged in Frog is history and stays, even
                // though Frog then reads further along than AniList.
                for (id, _) in run.iter().skip(upto) {
                    sqlx::query(
                        "DELETE FROM watch_event
                          WHERE user_id = ? AND episode_id = ? AND source = 'anilist'",
                    )
                    .bind(user_id)
                    .bind(id)
                    .execute(&state.pool)
                    .await?;
                }
            }
        }
    }
    Ok(existing.is_none())
}

/// A film AniList says was seen gets one undated watch, unless it has one.
async fn film_watch(pool: &SqlitePool, user_id: i64, title_id: i64) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch,
                                  source, undated)
         SELECT ?, ?, NULL, ?, 0, 'anilist', 1
          WHERE NOT EXISTS (SELECT 1 FROM watch_event
                             WHERE user_id = ? AND title_id = ? AND episode_id IS NULL)",
    )
    .bind(user_id)
    .bind(title_id)
    .bind(Utc::now().to_rfc3339())
    .bind(user_id)
    .bind(title_id)
    .execute(pool)
    .await?;
    Ok(())
}

// --- entries TMDB does not have -----------------------------------------------

/// The words TMDB uses for a show's state, for AniList's.
fn local_status(media_status: Option<&str>) -> Option<&'static str> {
    match media_status? {
        "FINISHED" => Some("Ended"),
        "RELEASING" | "HIATUS" => Some("Returning Series"),
        "NOT_YET_RELEASED" => Some("Planned"),
        "CANCELLED" => Some("Canceled"),
        _ => None,
    }
}

/// The AniList-only title an entry is kept as, if Frog made one.
async fn local_title_of(pool: &SqlitePool, media_id: i64) -> AppResult<Option<i64>> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM title WHERE anilist_id = ? AND tmdb_id < 0")
            .bind(media_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|r| r.0))
}

/// The entry as Frog has it while it is AniList-only: progress is a count,
/// since there are no episodes to tick.
async fn local_view(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    film: bool,
) -> AppResult<Option<View>> {
    let Some(entry) = store::get_entry(pool, user_id, title_id).await? else {
        return Ok(None);
    };
    let score = entry.rating.map(|r| r.round() as i64).unwrap_or(0);
    if film {
        let (watches,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM watch_event
              WHERE user_id = ? AND title_id = ? AND episode_id IS NULL",
        )
        .bind(user_id)
        .bind(title_id)
        .fetch_one(pool)
        .await?;
        let done = entry.status == "completed" || watches > 0;
        return Ok(Some(View {
            status: if done {
                "COMPLETED".into()
            } else {
                frog_to_anilist(&entry.status).into()
            },
            progress: i64::from(done),
            score,
        }));
    }
    let (progress,): (Option<i64>,) =
        sqlx::query_as("SELECT progress FROM library_entry WHERE user_id = ? AND title_id = ?")
            .bind(user_id)
            .bind(title_id)
            .fetch_one(pool)
            .await?;
    Ok(Some(View {
        status: frog_to_anilist(&entry.status).into(),
        progress: progress.unwrap_or(0),
        score,
    }))
}

/// Keep an entry TMDB cannot place (yet) in the library all the same, as a
/// title of its own drawn from AniList (`media`): poster, synopsis, cast,
/// episodes with their air dates. Where AniList lists the episodes, they are
/// ticked and synced like any show's; where it does not (a film, or a show
/// with nothing known yet), the count of episodes is kept instead. Status and
/// score go both ways. When TMDB places the entry later, `adopt_local`
/// moves it across.
async fn sync_local(
    state: &AppState,
    api: &Api<'_>,
    token: &str,
    user_id: i64,
    remote: &Remote,
    prior: Option<&Known>,
) -> AppResult<(bool, bool)> {
    let film = remote.format.as_deref() == Some("MOVIE");
    let kind = if film { "movie" } else { "tv" };
    let original = remote
        .titles
        .iter()
        .find(|t| **t != remote.name)
        .map(String::as_str);
    let title_id = store::ensure_local_title(
        &state.pool,
        &store::LocalTitle {
            anilist_id: remote.media_id,
            kind,
            name: &remote.name,
            original_name: original,
            release_date: remote.start.as_deref(),
            episodes: remote.episodes,
            status: local_status(remote.media_status.as_deref()),
        },
    )
    .await?;
    // The full picture from AniList, once; the worker keeps it fresh after.
    let (fresh,): (Option<String>,) = sqlx::query_as("SELECT refreshed_at FROM title WHERE id = ?")
        .bind(title_id)
        .fetch_one(&state.pool)
        .await?;
    if fresh.is_none() {
        if let Err(e) = store::ensure_title(state, kind, -remote.media_id).await {
            tracing::debug!(media_id = remote.media_id, error = %e, "AniList detail not read yet");
        }
    }
    let (episodes,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM episode WHERE title_id = ? AND season_number > 0")
            .bind(title_id)
            .fetch_one(&state.pool)
            .await?;
    if !film && episodes > 0 {
        return sync_local_episodes(state, api, token, user_id, remote, prior, title_id).await;
    }

    let a = &remote.view;
    // Snapshots of an entry that was placed before belong to that place.
    let (snap_al, snap_frog) = match prior {
        Some(k) if k.state == "unsure" => (k.al.clone(), k.frog.clone()),
        _ => (None, None),
    };
    let f = local_view(&state.pool, user_id, title_id, film).await?;
    if f.is_none() && snap_al.is_some() {
        // Taken out of the library in Frog: left alone from now on.
        save_snapshots(&state.pool, user_id, remote.media_id, a, None, true).await?;
        return Ok((false, false));
    }
    let first = snap_al.is_none();
    let al_changed = !first && snap_al.as_ref() != Some(a);
    let frog_changed = !first && f.as_ref() != snap_frog.as_ref();

    let incoming = anilist_to_frog(&a.status).to_string();
    if first || al_changed {
        let existing = store::get_entry(&state.pool, user_id, title_id).await?;
        let status = match &existing {
            Some(e) if first => merged_status(&e.status, &incoming),
            _ => incoming,
        };
        let rating = if a.score > 0 {
            match &existing {
                Some(e) if first && e.rating.is_some() => None,
                _ => Some(Some(a.score as f64)),
            }
        } else if al_changed && snap_al.as_ref().is_some_and(|b| b.score > 0) {
            Some(None)
        } else {
            None
        };
        let patch = LibraryPatch {
            status: Some(status),
            rating,
            ..Default::default()
        };
        store::apply_patch(&state.pool, user_id, title_id, &patch).await?;
        if film {
            if a.status == "COMPLETED" {
                film_watch(&state.pool, user_id, title_id).await?;
            }
        } else {
            // A count, not history: it follows AniList down as well as up.
            let have = f.as_ref().map(|f| f.progress).unwrap_or(0);
            let progress = if first {
                have.max(a.progress)
            } else {
                a.progress
            };
            store::set_local_progress(&state.pool, user_id, title_id, progress).await?;
        }
    }
    let f_now = local_view(&state.pool, user_id, title_id, film).await?;

    let mut final_al = a.clone();
    let mut pushed = false;
    if let (Some(now), true) = (f_now.as_ref(), frog_changed && !al_changed) {
        let send = View {
            status: now.status.clone(),
            progress: a.progress,
            score: now.score,
        };
        if &send != a {
            final_al = push(api, token, remote.media_id, &send).await?;
            pushed = true;
        }
    }
    save_snapshots(
        &state.pool,
        user_id,
        remote.media_id,
        &final_al,
        f_now.as_ref(),
        false,
    )
    .await?;
    Ok((f_now != f, pushed))
}

/// An AniList-only show whose episodes AniList lists: synced exactly as a
/// show placed on TMDB is, its one season being AniList's own numbering.
#[allow(clippy::too_many_arguments)]
async fn sync_local_episodes(
    state: &AppState,
    api: &Api<'_>,
    token: &str,
    user_id: i64,
    remote: &Remote,
    prior: Option<&Known>,
    title_id: i64,
) -> AppResult<(bool, bool)> {
    let target = Target::Show {
        tmdb_id: -remote.media_id,
        season: 1,
        offset: 0,
    };
    // Kept as a count before its episodes were known: laid out afresh, once.
    let (counted,): (Option<i64>,) =
        sqlx::query_as("SELECT progress FROM library_entry WHERE user_id = ? AND title_id = ?")
            .bind(user_id)
            .bind(title_id)
            .fetch_optional(&state.pool)
            .await?
            .unwrap_or((None,));
    let (snap_al, snap_frog) = match prior {
        Some(k) if k.state == "unsure" && counted.is_none() => (k.al.clone(), k.frog.clone()),
        _ => (None, None),
    };
    let a = &remote.view;
    let f = frog_view(
        &state.pool,
        user_id,
        remote.media_id,
        title_id,
        target,
        remote.episodes,
    )
    .await?;
    if f.is_none() && snap_al.is_some() {
        save_snapshots(&state.pool, user_id, remote.media_id, a, None, true).await?;
        return Ok((false, false));
    }
    let first = snap_al.is_none();
    let al_changed = !first && snap_al.as_ref() != Some(a);
    let frog_changed = !first && f.as_ref() != snap_frog.as_ref();
    let frog_progress = f.as_ref().map(|f| f.progress).unwrap_or(0);
    let follow = al_changed && (!frog_changed || a.progress >= frog_progress);
    let mode = if first {
        Some(Mode::Merge)
    } else if follow {
        Some(Mode::Follow)
    } else {
        None
    };
    match mode {
        Some(mode) => {
            let created = apply(state, user_id, remote, target, mode, snap_al.as_ref()).await?;
            let firm = if created { Mode::Follow } else { mode };
            settle_show(state, user_id, title_id, &[(1, 0, a.status.clone())], firm).await?;
        }
        None => release::reconcile(&state.pool, user_id, title_id).await?,
    }
    if counted.is_some() {
        sqlx::query("UPDATE library_entry SET progress = NULL WHERE user_id = ? AND title_id = ?")
            .bind(user_id)
            .bind(title_id)
            .execute(&state.pool)
            .await?;
    }
    let f_now = frog_view(
        &state.pool,
        user_id,
        remote.media_id,
        title_id,
        target,
        remote.episodes,
    )
    .await?;

    let mut final_al = a.clone();
    let mut pushed = false;
    if let Some(now) = f_now.as_ref() {
        let send = if first {
            // Only episodes watched in Frog that AniList has not seen.
            (now.progress > a.progress).then(|| View {
                status: now.status.clone(),
                progress: now.progress,
                score: a.score,
            })
        } else if frog_changed && !follow {
            let before = snap_frog.as_ref();
            let moved = |pick: fn(&View) -> String| before.map(pick) != Some(pick(now));
            Some(View {
                status: if moved(|v| v.status.clone()) {
                    now.status.clone()
                } else {
                    a.status.clone()
                },
                progress: if moved(|v| v.progress.to_string()) {
                    now.progress
                } else {
                    a.progress
                },
                score: if moved(|v| v.score.to_string()) {
                    now.score
                } else {
                    a.score
                },
            })
        } else {
            None
        };
        if let Some(send) = send.filter(|s| s != a) {
            final_al = push(api, token, remote.media_id, &send).await?;
            pushed = true;
        }
    }
    save_snapshots(
        &state.pool,
        user_id,
        remote.media_id,
        &final_al,
        f_now.as_ref(),
        false,
    )
    .await?;
    Ok((f_now != f, pushed))
}

/// An AniList-only title this account kept, once the entry is placed on a
/// TMDB title: its library entry, shelves and film watch move across. A
/// watch logged in Frog on one of its episodes goes to the episode at the
/// same place in the entry's run on TMDB, which is where the sync puts
/// AniList's own progress too; AniList's ticks are laid out afresh there.
async fn adopt_local(
    state: &AppState,
    user_id: i64,
    remote: &Remote,
    title_id: i64,
    target: Target,
) -> AppResult<bool> {
    let pool = &state.pool;
    let Some(local) = local_title_of(pool, remote.media_id).await? else {
        return Ok(false);
    };
    // (watch, episode number, source)
    let watches: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT w.id, e.episode_number, w.source FROM watch_event w
           JOIN episode e ON e.id = w.episode_id
          WHERE w.user_id = ? AND w.title_id = ? AND e.season_number = 1
          ORDER BY e.episode_number",
    )
    .bind(user_id)
    .bind(local)
    .fetch_all(pool)
    .await?;
    if let (
        false,
        Target::Show {
            tmdb_id,
            season,
            offset,
        },
    ) = (watches.is_empty(), target)
    {
        let (_, detail) = store::ensure_title(state, "tv", tmdb_id).await?;
        ensure_episodes(state, title_id, &detail).await?;
        let run = run_ids(
            pool,
            user_id,
            remote.media_id,
            title_id,
            season,
            offset,
            remote.episodes,
        )
        .await?;
        for (watch, number, source) in watches {
            let to = usize::try_from(number - 1).ok().and_then(|i| run.get(i));
            match (to, source.as_str()) {
                (_, "anilist") => {
                    sqlx::query("DELETE FROM watch_event WHERE id = ? AND user_id = ?")
                        .bind(watch)
                        .bind(user_id)
                        .execute(pool)
                        .await?;
                }
                (Some((episode, _)), _) => {
                    sqlx::query(
                        "UPDATE watch_event SET title_id = ?, episode_id = ?
                          WHERE id = ? AND user_id = ?",
                    )
                    .bind(title_id)
                    .bind(episode)
                    .bind(watch)
                    .bind(user_id)
                    .execute(pool)
                    .await?;
                }
                // Past the end of the run on TMDB: stays where it is, in the diary.
                (None, _) => {}
            }
        }
    }
    store::move_title_data(pool, user_id, local, title_id, "anilist: placed on TMDB").await
}

/// When to look for a place again after `tries` misses: an hour, doubling to
/// three days. Something not out yet is looked at twice a day.
fn next_try(tries: i64, upcoming: bool) -> String {
    let cap = if upcoming { 12 } else { 72 };
    let hours = (1i64 << tries.clamp(0, 7)).min(cap);
    (Utc::now() + chrono::Duration::hours(hours)).to_rfc3339()
}

/// Take back what an earlier, wrong placement of an entry ticked, so moving
/// it does not leave its progress behind in the wrong show or season. Only
/// ticks that came from AniList; anything ticked by hand stays.
async fn unplace(
    state: &AppState,
    user_id: i64,
    (media_id, episodes): (i64, Option<i64>),
    title_id: i64,
    old: Target,
    new: Option<(i64, Target)>,
) -> AppResult<()> {
    match old {
        Target::Movie(_) => {
            if new.is_some_and(|(t, _)| t == title_id) {
                return Ok(());
            }
            sqlx::query(
                "DELETE FROM watch_event
                  WHERE user_id = ? AND title_id = ? AND episode_id IS NULL AND source = 'anilist'",
            )
            .bind(user_id)
            .bind(title_id)
            .execute(&state.pool)
            .await?;
        }
        Target::Show { season, offset, .. } => {
            let old_run = run_ids(
                &state.pool,
                user_id,
                media_id,
                title_id,
                season,
                offset,
                episodes,
            )
            .await?;
            let mut keep: HashSet<i64> = match new {
                Some((t, Target::Show { season, offset, .. })) if t == title_id => {
                    run_ids(&state.pool, user_id, media_id, t, season, offset, episodes)
                        .await?
                        .into_iter()
                        .map(|r| r.0)
                        .collect()
                }
                _ => HashSet::new(),
            };
            // An episode another AniList entry of the same show covers keeps
            // its tick: that entry's progress is not this one's to take.
            let others: Vec<(i64, i64, i64, Option<i64>)> = sqlx::query_as(
                "SELECT media_id, season, episode_offset, episodes FROM anilist_entry
                  WHERE user_id = ? AND title_id = ? AND state = 'placed' AND season IS NOT NULL
                    AND media_id != ?",
            )
            .bind(user_id)
            .bind(title_id)
            .bind(media_id)
            .fetch_all(&state.pool)
            .await?;
            for (other, season, offset, length) in others {
                let run = run_ids(
                    &state.pool,
                    user_id,
                    other,
                    title_id,
                    season,
                    offset,
                    length,
                )
                .await?;
                keep.extend(run.into_iter().map(|r| r.0));
            }
            for (id, _) in old_run.iter().filter(|(id, _)| !keep.contains(id)) {
                sqlx::query(
                    "DELETE FROM watch_event
                      WHERE user_id = ? AND episode_id = ? AND source = 'anilist'",
                )
                .bind(user_id)
                .bind(id)
                .execute(&state.pool)
                .await?;
            }
        }
    }
    release::reconcile(&state.pool, user_id, title_id).await?;
    Ok(())
}

/// A show's one status, from the AniList entries that make it up and from
/// what has come out. Paused and dropped are AniList's word; the rest is what
/// has been watched against what has been released (`release`), so a planned
/// sequel never turns a finished season back into one being watched.
async fn settle_show(
    state: &AppState,
    user_id: i64,
    title_id: i64,
    entries: &[(i64, i64, String)],
    mode: Mode,
) -> AppResult<()> {
    let Some(entry) = store::get_entry(&state.pool, user_id, title_id).await? else {
        return Ok(());
    };
    // (season, offset, AniList status), in viewing order.
    let mut ordered = entries.to_vec();
    ordered.sort_by_key(|(s, o, _)| (*s, *o));
    let last_started = ordered.iter().rposition(|(_, _, st)| st != "PLANNING");
    let word = last_started.and_then(|i| match ordered[i].2.as_str() {
        "PAUSED" => Some("paused"),
        "DROPPED" => Some("dropped"),
        _ => None,
    });
    let status = match (word, mode) {
        (Some(w), Mode::Merge) => merged_status(&entry.status, w),
        (Some(w), Mode::Follow) => w.to_string(),
        // Taken up again on AniList.
        (None, Mode::Follow) if matches!(entry.status.as_str(), "paused" | "dropped") => {
            "watching".to_string()
        }
        _ => entry.status.clone(),
    };
    if status != entry.status {
        let patch = LibraryPatch {
            status: Some(status),
            ..Default::default()
        };
        store::apply_patch(&state.pool, user_id, title_id, &patch).await?;
    }
    release::reconcile(&state.pool, user_id, title_id).await
}

// --- one sync -----------------------------------------------------------------

#[derive(Debug, Default, Serialize)]
pub struct Summary {
    pub to_frog: i64,
    pub to_anilist: i64,
    pub unsure: i64,
    pub entries: i64,
    /// Not out yet, and not on TMDB yet: placed once they are.
    #[serde(default)]
    pub upcoming: i64,
    /// Placements moved to a better place this sync.
    #[serde(default)]
    pub repaired: i64,
    /// Kept in the library from AniList alone, with no TMDB title yet.
    #[serde(default)]
    pub local: i64,
    /// Entries that could not be written this time; tried again next sync.
    /// A sync with any is finished, but not complete.
    #[serde(default)]
    pub failed: i64,
    /// No longer on the AniList list. Kept in Frog as they are.
    #[serde(default)]
    pub removed: i64,
}

/// `sync_local` for one entry, counted, with a failure that is not an outage
/// kept to that entry.
async fn keep_local(
    state: &AppState,
    api: &Api<'_>,
    token: &str,
    user_id: i64,
    remote: &Remote,
    prior: Option<&Known>,
    summary: &mut Summary,
) -> AppResult<()> {
    match sync_local(state, api, token, user_id, remote, prior).await {
        Ok((to_frog, to_anilist)) => {
            summary.local += 1;
            summary.to_frog += i64::from(to_frog);
            summary.to_anilist += i64::from(to_anilist);
            Ok(())
        }
        Err(e) if is_outage(&e) => Err(e),
        Err(e) => {
            tracing::warn!(media_id = remote.media_id, error = %e, "keeping an AniList entry failed");
            summary.failed += 1;
            Ok(())
        }
    }
}

/// Entries gone from the AniList list are marked, and ones back unmarked.
/// Nothing in the library goes with them. A list that comes back empty when
/// Frog knows entries is more likely a hiccup than a cleared list, so then
/// nothing is marked at all.
async fn mark_gone(
    pool: &SqlitePool,
    user_id: i64,
    remotes: &[Remote],
    known: &HashMap<i64, Known>,
) -> AppResult<i64> {
    let present: HashSet<i64> = remotes.iter().map(|r| r.media_id).collect();
    let now = Utc::now().to_rfc3339();
    for (media_id, k) in known {
        if k.gone && present.contains(media_id) {
            sqlx::query(
                "UPDATE anilist_entry SET gone_at = NULL WHERE user_id = ? AND media_id = ?",
            )
            .bind(user_id)
            .bind(media_id)
            .execute(pool)
            .await?;
        }
    }
    if remotes.is_empty() && !known.is_empty() {
        tracing::warn!(
            user_id,
            known = known.len(),
            "AniList returned an empty list; nothing marked as removed"
        );
        return Ok(0);
    }
    let mut removed = 0;
    for (media_id, k) in known {
        if !k.gone && !present.contains(media_id) {
            sqlx::query("UPDATE anilist_entry SET gone_at = ? WHERE user_id = ? AND media_id = ?")
                .bind(&now)
                .bind(user_id)
                .bind(media_id)
                .execute(pool)
                .await?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// One stored entry, as the sync reads it back.
#[derive(sqlx::FromRow)]
struct EntryRow {
    media_id: i64,
    state: String,
    title_id: Option<i64>,
    kind: Option<String>,
    tmdb_id: Option<i64>,
    season: Option<i64>,
    episode_offset: i64,
    al_status: Option<String>,
    al_progress: Option<i64>,
    al_score: Option<i64>,
    frog_status: Option<String>,
    frog_progress: Option<i64>,
    frog_score: Option<i64>,
    detached: bool,
    manual: bool,
    resolver: i64,
    episodes: Option<i64>,
    format: Option<String>,
    media_status: Option<String>,
    start_date: Option<String>,
    tries: i64,
    next_try_at: Option<String>,
    gone_at: Option<String>,
}

struct Known {
    state: String,
    title_id: Option<i64>,
    target: Option<Target>,
    al: Option<View>,
    frog: Option<View>,
    detached: bool,
    /// Chosen by the person: kept, whatever the matcher thinks.
    manual: bool,
    resolver: i64,
    /// What AniList said about the media last time, to notice a change.
    meta: (Option<i64>, Option<String>, Option<String>, Option<String>),
    tries: i64,
    next_try_at: Option<String>,
    gone: bool,
}

impl Known {
    /// Not placed, tried recently, and nothing on AniList's side has changed
    /// since: looking again now would find what it found then.
    fn wait_to_place(&self, remote: &Remote, now: &str) -> bool {
        self.state == "unsure"
            && self.next_try_at.as_deref().is_some_and(|t| t > now)
            && self.meta
                == (
                    remote.episodes,
                    remote.format.clone(),
                    remote.media_status.clone(),
                    remote.start.clone(),
                )
    }
}

fn snapshot(status: Option<String>, progress: Option<i64>, score: Option<i64>) -> Option<View> {
    Some(View {
        status: status?,
        progress: progress.unwrap_or(0),
        score: score.unwrap_or(0),
    })
}

async fn known_entries(pool: &SqlitePool, user_id: i64) -> AppResult<HashMap<i64, Known>> {
    let rows: Vec<EntryRow> = sqlx::query_as(
        "SELECT a.media_id, a.state, a.title_id, t.kind, t.tmdb_id, a.season, a.episode_offset,
                a.al_status, a.al_progress, a.al_score,
                a.frog_status, a.frog_progress, a.frog_score, a.detached, a.manual, a.resolver,
                a.episodes, a.format, a.media_status, a.start_date, a.tries, a.next_try_at,
                a.gone_at
           FROM anilist_entry a LEFT JOIN title t ON t.id = a.title_id
          WHERE a.user_id = ?",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let mut out = HashMap::new();
    for r in rows {
        let target = match (r.kind.as_deref(), r.tmdb_id) {
            (Some("movie"), Some(id)) => Some(Target::Movie(id)),
            (Some("tv"), Some(id)) => r.season.map(|season| Target::Show {
                tmdb_id: id,
                season,
                offset: r.episode_offset,
            }),
            _ => None,
        };
        out.insert(
            r.media_id,
            Known {
                state: r.state,
                title_id: r.title_id,
                target,
                al: snapshot(r.al_status, r.al_progress, r.al_score),
                frog: snapshot(r.frog_status, r.frog_progress, r.frog_score),
                detached: r.detached,
                manual: r.manual,
                resolver: r.resolver,
                meta: (r.episodes, r.format, r.media_status, r.start_date),
                tries: r.tries,
                next_try_at: r.next_try_at,
                gone: r.gone_at.is_some(),
            },
        );
    }
    Ok(out)
}

/// How an entry was (or was not) placed, as Settings shows it.
struct Verdict<'a> {
    state: &'a str,
    outcome: &'a str,
    method: Option<&'a str>,
    confidence: i64,
    note: Option<String>,
}

async fn save_entry(
    pool: &SqlitePool,
    user_id: i64,
    remote: &Remote,
    verdict: &Verdict<'_>,
    placed: Option<(i64, Target)>,
    candidates: Option<&[Value]>,
) -> AppResult<()> {
    let (title_id, season, offset) = match placed {
        Some((id, Target::Show { season, offset, .. })) => (Some(id), Some(season), offset),
        Some((id, Target::Movie(_))) => (Some(id), None, 0),
        None => (None, None, 0),
    };
    let next_airing_at = remote
        .next_airing
        .and_then(|(_, at)| chrono::DateTime::from_timestamp(at, 0))
        .map(|t| t.to_rfc3339());
    sqlx::query(
        "INSERT INTO anilist_entry (user_id, media_id, name, format, episodes, state, title_id,
                                    season, episode_offset, candidates_json, updated_at,
                                    outcome, method, confidence, note, resolver, media_status,
                                    start_date, next_airing_at, next_airing_episode)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (user_id, media_id) DO UPDATE SET
            name = excluded.name, format = excluded.format, episodes = excluded.episodes,
            state = excluded.state, title_id = excluded.title_id, season = excluded.season,
            episode_offset = excluded.episode_offset,
            candidates_json = CASE WHEN excluded.candidates_json = '[]'
                                   THEN anilist_entry.candidates_json
                                   ELSE excluded.candidates_json END,
            updated_at = excluded.updated_at, outcome = excluded.outcome,
            method = excluded.method, confidence = excluded.confidence, note = excluded.note,
            resolver = excluded.resolver, media_status = excluded.media_status,
            start_date = excluded.start_date, next_airing_at = excluded.next_airing_at,
            next_airing_episode = excluded.next_airing_episode,
            tries = CASE WHEN excluded.state = 'placed' THEN 0 ELSE anilist_entry.tries END,
            next_try_at = CASE WHEN excluded.state = 'placed' THEN NULL
                               ELSE anilist_entry.next_try_at END",
    )
    .bind(user_id)
    .bind(remote.media_id)
    .bind(&remote.name)
    .bind(&remote.format)
    .bind(remote.episodes)
    .bind(verdict.state)
    .bind(title_id)
    .bind(season)
    .bind(offset)
    .bind(serde_json::to_string(&candidates.unwrap_or(&[]))?)
    .bind(Utc::now().to_rfc3339())
    .bind(verdict.outcome)
    .bind(verdict.method)
    .bind(verdict.confidence)
    .bind(&verdict.note)
    .bind(place::RESOLVER)
    .bind(&remote.media_status)
    .bind(&remote.start)
    .bind(next_airing_at)
    .bind(remote.next_airing.map(|(ep, _)| ep))
    .execute(pool)
    .await?;
    Ok(())
}

async fn save_snapshots(
    pool: &SqlitePool,
    user_id: i64,
    media_id: i64,
    al: &View,
    frog: Option<&View>,
    detached: bool,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE anilist_entry
            SET al_status = ?, al_progress = ?, al_score = ?,
                frog_status = ?, frog_progress = ?, frog_score = ?,
                detached = ?, updated_at = ?
          WHERE user_id = ? AND media_id = ?",
    )
    .bind(&al.status)
    .bind(al.progress)
    .bind(al.score)
    .bind(frog.map(|f| f.status.clone()))
    .bind(frog.map(|f| f.progress))
    .bind(frog.map(|f| f.score))
    .bind(detached)
    .bind(Utc::now().to_rfc3339())
    .bind(user_id)
    .bind(media_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn push(api: &Api<'_>, token: &str, media_id: i64, view: &View) -> AppResult<View> {
    let data = api
        .graphql(
            token,
            SAVE_MUTATION,
            json!({
                "mediaId": media_id,
                "status": view.status,
                "progress": view.progress,
                "scoreRaw": view.score,
            }),
        )
        .await?;
    let saved = &data["SaveMediaListEntry"];
    Ok(View {
        status: saved["status"]
            .as_str()
            .unwrap_or(&view.status)
            .replace("REPEATING", "CURRENT"),
        progress: saved["progress"].as_i64().unwrap_or(view.progress),
        score: saved["score"]
            .as_f64()
            .map(|s| s.round() as i64)
            .unwrap_or(view.score),
    })
}

// --- looking without touching anything -------------------------------------

/// Somebody's AniList list by name, read anonymously. Public lists only.
pub async fn public_list(state: &AppState, user_name: &str) -> AppResult<Vec<Remote>> {
    let api = Api::new(state)?;
    let query = LIST_QUERY
        .replace("$userId: Int", "$userName: String")
        .replace("userId: $userId", "userName: $userName");
    fetch_list(&api, "", &query, json!({ "userName": user_name })).await
}

/// Where each entry would go, how sure that is and why, without writing to
/// anyone's library: (media id, name, outcome, place, method, confidence,
/// note).
pub async fn diagnose(state: &AppState, remotes: &[Remote]) -> AppResult<Vec<Value>> {
    let api = Api::new(state)?;
    if let Some(client) = state.config.anilist.as_ref() {
        ensure_map(state, client).await?;
    }
    let mut out = Vec::new();
    for remote in remotes {
        let row = match place::place(state, &api, "", remote).await {
            Ok(p) => json!({
                "media_id": remote.media_id,
                "name": remote.name,
                "format": remote.format,
                "media_status": remote.media_status,
                "outcome": p.outcome.as_str(),
                "place": p.target.map(|t| match t {
                    Target::Movie(id) => format!("film {id}"),
                    Target::Show { tmdb_id, season, offset } => {
                        format!("show {tmdb_id} S{season} from E{}", offset + 1)
                    }
                }),
                "method": p.method,
                "confidence": p.confidence,
                "note": p.note,
                "candidates": p.candidates,
                "evidence": p.evidence,
            }),
            Err(e) => json!({
                "media_id": remote.media_id,
                "name": remote.name,
                "outcome": "error",
                "note": e.to_string(),
            }),
        };
        out.push(row);
    }
    Ok(out)
}

/// Sync one account, start to finish. Records the outcome on the link.
pub async fn sync_user(state: &AppState, user_id: i64) -> AppResult<Summary> {
    let Some(_running) = RunningGuard::claim(user_id) else {
        return Ok(Summary::default());
    };
    let result = sync_inner(state, user_id).await;
    let now = Utc::now().to_rfc3339();
    match &result {
        // Only a sync that finished moves synced_at: it is the checkpoint
        // Settings shows and the worker counts from.
        Ok(summary) => {
            sqlx::query(
                "UPDATE anilist_link SET synced_at = ?, attempted_at = ?, dirty = 0,
                        last_error = NULL, failures = 0, retry_at = NULL, needs_reconnect = 0,
                        last_summary = ? WHERE user_id = ?",
            )
            .bind(&now)
            .bind(&now)
            .bind(serde_json::to_string(summary)?)
            .bind(user_id)
            .execute(&state.pool)
            .await?;
        }
        Err(e) => {
            let signed_out = matches!(e, AppError::Forbidden(_));
            let message = match e {
                AppError::Forbidden(m) | AppError::BadRequest(m) => m.clone(),
                AppError::Tmdb(_) | AppError::TmdbUnconfigured => {
                    "Could not reach TMDB to place your titles. Frog will try again shortly."
                        .to_string()
                }
                _ => "Could not reach AniList. Frog will try again shortly.".to_string(),
            };
            let (failures,): (i64,) =
                sqlx::query_as("SELECT failures FROM anilist_link WHERE user_id = ?")
                    .bind(user_id)
                    .fetch_optional(&state.pool)
                    .await?
                    .unwrap_or((0,));
            let retry_at = retry_after(failures);
            tracing::warn!(user_id, failures = failures + 1, signed_out, error = %e, "AniList sync failed");
            sqlx::query(
                "UPDATE anilist_link SET attempted_at = ?, last_error = ?, failures = failures + 1,
                        retry_at = ?, needs_reconnect = ? WHERE user_id = ?",
            )
            .bind(&now)
            .bind(message)
            .bind(retry_at)
            .bind(signed_out)
            .bind(user_id)
            .execute(&state.pool)
            .await?;
        }
    }
    result
}

/// When a failing account is tried again: one minute, doubling up to an
/// hour, give or take a fifth so accounts do not move in step.
fn retry_after(failures: i64) -> String {
    let minutes = (1i64 << failures.clamp(0, 6)).min(MAX_BACKOFF_MINUTES) as f64;
    let jitter: f64 = {
        use rand::Rng;
        rand::thread_rng().gen_range(0.8..1.2)
    };
    let seconds = (minutes * 60.0 * jitter) as i64;
    (Utc::now() + chrono::Duration::seconds(seconds)).to_rfc3339()
}

/// TMDB out of reach, as opposed to something wrong with one entry: the
/// whole sync stops and is tried again, so nothing is marked failed for it.
fn is_outage(e: &AppError) -> bool {
    matches!(
        e,
        AppError::Tmdb(_) | AppError::TmdbUnconfigured | AppError::AniList(_)
    )
}

async fn sync_inner(state: &AppState, user_id: i64) -> AppResult<Summary> {
    let api = Api::new(state)?;
    let link: Option<(i64, String)> =
        sqlx::query_as("SELECT anilist_user_id, token FROM anilist_link WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(&state.pool)
            .await?;
    let Some((anilist_user_id, token)) = link else {
        return Ok(Summary::default());
    };
    ensure_map(state, api.client).await?;

    let remotes = fetch_list(
        &api,
        &token,
        LIST_QUERY,
        json!({ "userId": anilist_user_id }),
    )
    .await?;
    for r in &remotes {
        remember_media(
            &state.pool,
            r.media_id,
            r.episodes,
            r.format.as_deref(),
            &r.name,
        )
        .await?;
    }
    let known = known_entries(&state.pool, user_id).await?;
    let started = Utc::now().to_rfc3339();

    let mut summary = Summary {
        entries: remotes.len() as i64,
        ..Default::default()
    };
    summary.removed = mark_gone(&state.pool, user_id, &remotes, &known).await?;
    // Shows changed from AniList's side, and how firmly.
    let mut shows: HashMap<i64, Mode> = HashMap::new();
    // Every show an entry is placed in, so each is settled once at the end.
    let mut touched: HashSet<i64> = HashSet::new();

    for remote in &remotes {
        let prior = known.get(&remote.media_id);
        if prior.is_some_and(|k| k.state == "ignored" || k.detached) {
            continue;
        }
        let prior_place =
            prior.and_then(|k| k.target.zip(k.title_id).filter(|_| k.state == "placed"));

        // Not placed last time and nothing new to go on: it stays as it is in
        // the library until its next look, which keeps a short sync short.
        if prior.is_some_and(|k| k.wait_to_place(remote, &started)) {
            keep_local(state, &api, &token, user_id, remote, prior, &mut summary).await?;
            continue;
        }

        // A place the person chose, or one the current matcher made, stays.
        // Anything older is looked at again and moved only when the new
        // place is certain.
        let keep = prior.is_some_and(|k| k.manual || k.resolver >= place::RESOLVER);
        let (target, verdict, candidates): (Option<Target>, Verdict, Vec<Value>) = match (
            prior_place,
            keep,
        ) {
            (Some((t, _)), true) => (
                Some(t),
                Verdict {
                    state: "placed",
                    outcome: "matched",
                    method: None,
                    confidence: -1,
                    note: None,
                },
                Vec::new(),
            ),
            _ => {
                let p = match place::place(state, &api, &token, remote).await {
                    Ok(p) => p,
                    Err(e) if is_outage(&e) => return Err(e),
                    Err(e) => {
                        // One entry Frog cannot look up does not hold up
                        // the rest; it is looked at again next time.
                        tracing::warn!(media_id = remote.media_id, error = %e, "placing an AniList entry failed");
                        summary.failed += 1;
                        continue;
                    }
                };
                match (p.target, prior_place) {
                    (Some(t), _) => (
                        Some(t),
                        Verdict {
                            state: "placed",
                            outcome: "matched",
                            method: p.method,
                            confidence: p.confidence,
                            note: p.note,
                        },
                        Vec::new(),
                    ),
                    // Placed before, and the matcher can no longer say:
                    // leave it be rather than take progress away.
                    (None, Some((old, _))) => (
                        Some(old),
                        Verdict {
                            state: "placed",
                            outcome: "matched",
                            method: Some("kept"),
                            confidence: p.confidence.min(CONFIDENT - 1),
                            note: Some(
                                "Kept where it was. Frog could not confirm it, so check it.".into(),
                            ),
                        },
                        p.candidates,
                    ),
                    (None, None) => {
                        match p.outcome {
                            Outcome::Upcoming => summary.upcoming += 1,
                            _ => summary.unsure += 1,
                        }
                        let show_title = match p.show {
                            Some(id) => store::find_title_id(&state.pool, "tv", id).await?,
                            None => None,
                        };
                        let verdict = Verdict {
                            state: "unsure",
                            outcome: p.outcome.as_str(),
                            method: p.method,
                            confidence: p.confidence,
                            note: p.note,
                        };
                        save_entry(
                            &state.pool,
                            user_id,
                            remote,
                            &verdict,
                            None,
                            Some(&p.candidates),
                        )
                        .await?;
                        if let Some(t) = show_title {
                            sqlx::query(
                                "UPDATE anilist_entry SET title_id = ?
                                      WHERE user_id = ? AND media_id = ?",
                            )
                            .bind(t)
                            .bind(user_id)
                            .bind(remote.media_id)
                            .execute(&state.pool)
                            .await?;
                        }
                        // Looked for again later, sooner when it is not
                        // out yet, and at once if AniList changes it.
                        let tries = prior
                            .filter(|k| k.meta.2 == remote.media_status)
                            .map(|k| k.tries)
                            .unwrap_or(0);
                        sqlx::query(
                            "UPDATE anilist_entry SET tries = ?, next_try_at = ?
                                  WHERE user_id = ? AND media_id = ?",
                        )
                        .bind(tries + 1)
                        .bind(next_try(tries, p.outcome == Outcome::Upcoming))
                        .bind(user_id)
                        .bind(remote.media_id)
                        .execute(&state.pool)
                        .await?;
                        keep_local(state, &api, &token, user_id, remote, prior, &mut summary)
                            .await?;
                        continue;
                    }
                }
            }
        };
        let Some(target) = target else { continue };

        let (kind, tmdb_id) = match target {
            Target::Movie(id) => ("movie", id),
            Target::Show { tmdb_id, .. } => ("tv", tmdb_id),
        };
        let title_id = match prior_place {
            Some((old, id)) if old == target => id,
            _ => store::ensure_title(state, kind, tmdb_id).await?.0,
        };

        // Moved: take back what the old place was given, then start this
        // entry afresh where it belongs.
        let moved = prior_place.filter(|(old, _)| *old != target);
        if let Some((old, old_title)) = moved {
            unplace(
                state,
                user_id,
                (remote.media_id, remote.episodes),
                old_title,
                old,
                Some((title_id, target)),
            )
            .await?;
            summary.repaired += 1;
        }
        // Kept as an AniList-only title until now: what this account has
        // there moves to the real title, which then starts afresh.
        let adopted = match adopt_local(state, user_id, remote, title_id, target).await {
            Ok(moved) => moved,
            Err(e) => {
                tracing::warn!(media_id = remote.media_id, error = %e, "moving an AniList-only title failed");
                summary.failed += 1;
                continue;
            }
        };
        let fresh = moved.is_some() || adopted || prior.is_some_and(|k| k.state != "placed");
        let (snap_al, snap_frog) = if fresh {
            (None, None)
        } else {
            (
                prior.and_then(|k| k.al.clone()),
                prior.and_then(|k| k.frog.clone()),
            )
        };

        // The verdict of a kept placement stays as it was.
        if verdict.confidence >= 0 {
            save_entry(
                &state.pool,
                user_id,
                remote,
                &verdict,
                Some((title_id, target)),
                Some(&candidates),
            )
            .await?;
        } else {
            sqlx::query(
                "UPDATE anilist_entry SET media_status = ?, start_date = ?, episodes = ?,
                        updated_at = ?, tries = 0, next_try_at = NULL
                  WHERE user_id = ? AND media_id = ?",
            )
            .bind(&remote.media_status)
            .bind(&remote.start)
            .bind(remote.episodes)
            .bind(Utc::now().to_rfc3339())
            .bind(user_id)
            .bind(remote.media_id)
            .execute(&state.pool)
            .await?;
        }
        if kind == "tv" {
            touched.insert(title_id);
        }

        let a = &remote.view;
        let f = frog_view(
            &state.pool,
            user_id,
            remote.media_id,
            title_id,
            target,
            remote.episodes,
        )
        .await?;

        // Removed from Frog after it was synced: leave it be from now on.
        if f.is_none() && snap_al.is_some() {
            save_snapshots(&state.pool, user_id, remote.media_id, a, None, true).await?;
            continue;
        }

        let first = snap_al.is_none();
        // Placed again by a newer matcher: AniList's progress is laid out
        // afresh where the entry now runs, which also fills in what an older,
        // shorter run had cut off.
        let replaced =
            verdict.confidence >= 0 && prior.is_some_and(|k| k.resolver < place::RESOLVER);
        let al_changed = !first && snap_al.as_ref() != Some(a);
        let frog_changed = !first && f.as_ref() != snap_frog.as_ref();
        let frog_progress = f.as_ref().map(|f| f.progress).unwrap_or(0);
        // Both sides moved: whoever is further along wins.
        let follow = al_changed && (!frog_changed || a.progress >= frog_progress);

        let mode = if first || replaced {
            Some(Mode::Merge)
        } else if follow {
            Some(Mode::Follow)
        } else {
            None
        };
        if let Some(mode) = mode {
            let created = match apply(state, user_id, remote, target, mode, snap_al.as_ref()).await
            {
                Ok(created) => created,
                Err(e) if is_outage(&e) => return Err(e),
                Err(e) => {
                    // One entry that cannot be written does not stop the rest;
                    // Settings says which and why.
                    tracing::warn!(media_id = remote.media_id, error = %e, "applying an AniList entry failed");
                    sqlx::query(
                        "UPDATE anilist_entry SET outcome = 'apply_failed', note = ?
                          WHERE user_id = ? AND media_id = ?",
                    )
                    .bind("Placed, but Frog could not write it. It tries again at the next sync.")
                    .bind(user_id)
                    .bind(remote.media_id)
                    .execute(&state.pool)
                    .await?;
                    // Not remembered as synced, so the next sync tries again.
                    sqlx::query(
                        "UPDATE anilist_entry SET resolver = 0 WHERE user_id = ? AND media_id = ?",
                    )
                    .bind(user_id)
                    .bind(remote.media_id)
                    .execute(&state.pool)
                    .await?;
                    continue;
                }
            };
            if kind == "tv" {
                // New to the library, or changed on AniList: AniList decides.
                let firm = if created { Mode::Follow } else { mode };
                let slot = shows.entry(title_id).or_insert(firm);
                if firm == Mode::Follow {
                    *slot = Mode::Follow;
                }
            }
        }

        let f_now = frog_view(
            &state.pool,
            user_id,
            remote.media_id,
            title_id,
            target,
            remote.episodes,
        )
        .await?;
        if f_now != f {
            summary.to_frog += 1;
        }

        // Frog has something AniList does not: send it.
        let mut final_al = a.clone();
        if let Some(f_now) = f_now.as_ref() {
            let send = if first || replaced {
                // A first sync only sends episodes watched in Frog that
                // AniList has not seen. Scores wait for a real change.
                (f_now.progress > a.progress).then(|| View {
                    status: f_now.status.clone(),
                    progress: f_now.progress,
                    score: a.score,
                })
            } else if frog_changed && !follow {
                // Only what changed in Frog; the rest stays as AniList has it.
                let before = snap_frog.as_ref();
                let moved = |pick: fn(&View) -> String| before.map(pick) != Some(pick(f_now));
                Some(View {
                    status: if moved(|v| v.status.clone()) {
                        f_now.status.clone()
                    } else {
                        a.status.clone()
                    },
                    progress: if moved(|v| v.progress.to_string()) {
                        f_now.progress
                    } else {
                        a.progress
                    },
                    score: if moved(|v| v.score.to_string()) {
                        f_now.score
                    } else {
                        a.score
                    },
                })
            } else {
                None
            };
            if let Some(send) = send.filter(|s| s != a) {
                final_al = push(&api, &token, remote.media_id, &send).await?;
                summary.to_anilist += 1;
            }
        }
        save_snapshots(
            &state.pool,
            user_id,
            remote.media_id,
            &final_al,
            f_now.as_ref(),
            false,
        )
        .await?;
    }

    // One status per show, once all of its seasons are in. Shows AniList
    // changed take AniList's paused or dropped; every show is then held to
    // what has come out, which is also what repairs a status an earlier sync
    // got wrong.
    for title_id in touched {
        let rows: Vec<(i64, i64, String, i64)> = sqlx::query_as(
            "SELECT COALESCE(season, 0), episode_offset, COALESCE(al_status, 'PLANNING'), media_id
               FROM anilist_entry
              WHERE user_id = ? AND title_id = ? AND state = 'placed' AND detached = 0",
        )
        .bind(user_id)
        .bind(title_id)
        .fetch_all(&state.pool)
        .await?;
        match shows.get(&title_id) {
            Some(&mode) => {
                let entries: Vec<(i64, i64, String)> =
                    rows.iter().map(|r| (r.0, r.1, r.2.clone())).collect();
                settle_show(state, user_id, title_id, &entries, mode).await?;
            }
            None => release::reconcile(&state.pool, user_id, title_id).await?,
        }
        // Now the Frog side of each of those entries is final.
        for (season, offset, _, media_id) in rows {
            let Some(remote) = remotes.iter().find(|r| r.media_id == media_id) else {
                continue;
            };
            let target = Target::Show {
                tmdb_id: 0,
                season,
                offset,
            };
            let f = frog_view(
                &state.pool,
                user_id,
                media_id,
                title_id,
                target,
                remote.episodes,
            )
            .await?;
            sqlx::query(
                "UPDATE anilist_entry SET frog_status = ?, frog_progress = ?, frog_score = ?
                  WHERE user_id = ? AND media_id = ?",
            )
            .bind(f.as_ref().map(|f| f.status.clone()))
            .bind(f.as_ref().map(|f| f.progress))
            .bind(f.as_ref().map(|f| f.score))
            .bind(user_id)
            .bind(media_id)
            .execute(&state.pool)
            .await?;
        }
    }

    Ok(summary)
}

/// Accounts whose list is due: changed in Frog, never synced, or idle long
/// enough to look again, and not waiting out a failure. It all lives in the
/// database, so a restart picks up where the last run left off. Expired or
/// refused tokens are skipped; Settings asks for a reconnect instead.
pub async fn due(pool: &SqlitePool) -> AppResult<Vec<i64>> {
    let idle = (Utc::now() - chrono::Duration::minutes(sync_minutes())).to_rfc3339();
    let settle = (Utc::now() - chrono::Duration::seconds(45)).to_rfc3339();
    let now = Utc::now().to_rfc3339();
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT user_id FROM anilist_link
          WHERE expires_at > ?1 AND needs_reconnect = 0
            AND (retry_at IS NULL OR retry_at <= ?1)
            AND (synced_at IS NULL OR synced_at < ?2 OR (dirty = 1 AND synced_at < ?3))
          ORDER BY COALESCE(synced_at, '')",
    )
    .bind(now)
    .bind(idle)
    .bind(settle)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rewatch_reads_as_watching_and_duplicates_are_dropped() {
        let data = json!({ "MediaListCollection": { "lists": [
            { "entries": [
                { "status": "REPEATING", "score": 85.0, "progress": 3,
                  "media": { "id": 1, "format": "TV", "episodes": 12,
                             "title": { "english": null, "romaji": "Kimi" } } }
            ] },
            { "entries": [
                { "status": "CURRENT", "score": 85.0, "progress": 3,
                  "media": { "id": 1, "format": "TV", "episodes": 12,
                             "title": { "english": null, "romaji": "Kimi" } } }
            ] }
        ] } });
        let list = parse_list(&data);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].view.status, "CURRENT");
        assert_eq!(list[0].name, "Kimi");
        assert_eq!(list[0].view.score, 85);
    }
}
