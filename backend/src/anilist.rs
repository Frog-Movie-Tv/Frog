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

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::config::AniListClient;
use crate::error::{AppError, AppResult};
use crate::routes::transfer::merged_status;
use crate::state::AppState;
use crate::store::{self, LibraryPatch};
use crate::tmdb::models;

/// AniList allows 30 requests a minute at the moment; stay under it.
const SPACING: Duration = Duration::from_millis(2_100);

/// How often the worker re-reads a list nobody touched. One request per
/// account, so a change made on AniList shows up in Frog within minutes.
pub const IDLE_SYNC_MINUTES: i64 = 5;

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

    /// One GraphQL call, paced and retried once after a rate limit.
    pub async fn graphql(&self, token: &str, query: &str, variables: Value) -> AppResult<Value> {
        for attempt in 0..2 {
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
            let resp = req
                .header("accept", "application/json")
                .json(&json!({ "query": query, "variables": variables }))
                .send()
                .await
                .map_err(|e| AppError::AniList(e.to_string()))?;

            let status = resp.status();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS && attempt == 0 {
                let secs = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(60)
                    .min(65);
                tokio::time::sleep(Duration::from_secs(secs)).await;
                continue;
            }
            if status == reqwest::StatusCode::UNAUTHORIZED {
                return Err(AppError::Forbidden(
                    "AniList signed Frog out. Connect it again in Settings.".into(),
                ));
            }
            let body: Value = resp
                .json()
                .await
                .map_err(|e| AppError::AniList(format!("bad JSON: {e}")))?;
            if let Some(err) = body["errors"].as_array().and_then(|a| a.first()) {
                let message = err["message"].as_str().unwrap_or("unknown error");
                if message.to_lowercase().contains("invalid token") {
                    return Err(AppError::Forbidden(
                        "AniList signed Frog out. Connect it again in Settings.".into(),
                    ));
                }
                return Err(AppError::AniList(format!("{status}: {message}")));
            }
            if !status.is_success() {
                return Err(AppError::AniList(status.to_string()));
            }
            return Ok(body["data"].clone());
        }
        Err(AppError::AniList("rate limited".into()))
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

const LIST_QUERY: &str = "query ($userId: Int) {
  MediaListCollection(userId: $userId, type: ANIME) {
    lists { entries {
      status
      score(format: POINT_100)
      progress
      media { id format episodes seasonYear startDate { year } title { english romaji native } }
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
    pub format: Option<String>,
    pub episodes: Option<i64>,
    pub year: Option<i64>,
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
            let mut titles: Vec<String> = Vec::new();
            for key in ["english", "romaji", "native"] {
                if let Some(t) = media["title"][key].as_str().map(str::trim) {
                    if !t.is_empty() && !titles.iter().any(|known| known == t) {
                        titles.push(t.to_string());
                    }
                }
            }
            out.push(Remote {
                media_id,
                name: media_name(media),
                titles,
                format: media["format"].as_str().map(str::to_string),
                episodes: media["episodes"].as_i64().filter(|n| *n > 0),
                // The year it started, which for a film is the year it came
                // out; seasonYear is missing for most films.
                year: media["startDate"]["year"]
                    .as_i64()
                    .or_else(|| media["seasonYear"].as_i64()),
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

/// (tvdb_id, tvdb_season, tvdb_epoffset, imdb_id,
///  tmdb_kind, tmdb_id, tmdb_season, tmdb_offset)
type MapRow = (
    Option<i64>,
    Option<i64>,
    i64,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
);

/// Where an AniList entry lives in Frog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Movie(i64),
    Show {
        tmdb_id: i64,
        season: i64,
        offset: i64,
    },
}

/// Find the TMDB season and offset that absolute episode `abs` (0-based) falls
/// in, given TMDB's (season, episode count) list. Specials are not counted.
pub fn locate(abs: i64, seasons: &[(i64, i64)]) -> Option<(i64, i64)> {
    let mut remaining = abs;
    for &(number, count) in seasons.iter().filter(|(n, _)| *n >= 1) {
        if remaining < count {
            return Some((number, remaining));
        }
        remaining -= count;
    }
    None
}

/// Place a TVDB (season, offset) on TMDB's numbering.
///
/// When TMDB has the same season and it is long enough, the numbering is the
/// same. Otherwise `earlier` is how many episodes TVDB's earlier seasons hold,
/// which gives the absolute position to find in TMDB's seasons. Season -1 is
/// TVDB's absolute order, where the offset already is the position.
pub fn place_tv(
    tvdb_season: i64,
    offset: i64,
    episodes: Option<i64>,
    tmdb_seasons: &[(i64, i64)],
    earlier: Option<i64>,
) -> Option<(i64, i64)> {
    if tvdb_season == -1 {
        return locate(offset, tmdb_seasons);
    }
    if tvdb_season < 1 {
        return None;
    }
    let needed = offset + episodes.unwrap_or(1).max(1);
    if tmdb_seasons
        .iter()
        .any(|&(n, count)| n == tvdb_season && count >= needed)
    {
        return Some((tvdb_season, offset));
    }
    locate(earlier? + offset, tmdb_seasons)
}

/// Episode counts for AniList ids, from the shared cache or AniList itself.
async fn media_episodes(
    state: &AppState,
    api: &Api<'_>,
    token: &str,
    ids: &[i64],
) -> AppResult<HashMap<i64, Option<i64>>> {
    let mut out = HashMap::new();
    let mut missing = Vec::new();
    for &id in ids {
        let row: Option<(Option<i64>,)> =
            sqlx::query_as("SELECT episodes FROM anilist_media WHERE id = ?")
                .bind(id)
                .fetch_optional(&state.pool)
                .await?;
        match row {
            Some((n,)) => {
                out.insert(id, n);
            }
            None => missing.push(id),
        }
    }
    for chunk in missing.chunks(50) {
        let data = api
            .graphql(token, MEDIA_QUERY, json!({ "ids": chunk }))
            .await?;
        for m in data["Page"]["media"]
            .as_array()
            .cloned()
            .unwrap_or_default()
        {
            let Some(id) = m["id"].as_i64() else { continue };
            let n = m["episodes"].as_i64().filter(|n| *n > 0);
            remember_media(&state.pool, id, n, m["format"].as_str(), &media_name(&m)).await?;
            out.insert(id, n);
        }
    }
    Ok(out)
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

/// The TMDB title another database's id points at. `Ok(None)` is TMDB saying
/// it knows no such id; TMDB being unreachable is an error, so a bad moment
/// never turns a whole list into "needs a place".
async fn tmdb_by_external(
    state: &AppState,
    id: &str,
    source: &str,
) -> AppResult<Option<(String, i64)>> {
    let found = match state.tmdb.find(id, source).await {
        Ok(v) => v,
        Err(AppError::NotFound(_)) => return Ok(None),
        Err(e) => return Err(e),
    };
    let first = |key: &str| found[key].as_array().and_then(|a| a.first()).cloned();
    if let Some(m) = first("movie_results") {
        return Ok(m["id"].as_i64().map(|id| ("movie".into(), id)));
    }
    if let Some(t) = first("tv_results") {
        return Ok(t["id"].as_i64().map(|id| ("tv".into(), id)));
    }
    // An episode's own id still says which show it belongs to.
    if let Some(e) = first("tv_episode_results") {
        return Ok(e["show_id"].as_i64().map(|id| ("tv".into(), id)));
    }
    Ok(None)
}

/// Where this entry goes, or `None` when that is not certain. By TMDB id
/// first, then by TVDB or IMDb id, then by an exact name, and last by
/// counting on from the first season of the series it continues.
async fn place(
    state: &AppState,
    api: &Api<'_>,
    token: &str,
    remote: &Remote,
) -> AppResult<Option<Target>> {
    if let Some(target) = place_known(state, api, token, remote).await? {
        return Ok(Some(target));
    }
    place_after_prequels(state, api, token, remote).await
}

/// By id from either list or, for what neither list knows, by exact name.
async fn place_known(
    state: &AppState,
    api: &Api<'_>,
    token: &str,
    remote: &Remote,
) -> AppResult<Option<Target>> {
    let row: Option<MapRow> = sqlx::query_as(
        "SELECT tvdb_id, tvdb_season, tvdb_epoffset, imdb_id,
                tmdb_kind, tmdb_id, tmdb_season, tmdb_offset
           FROM anime_map WHERE anilist_id = ?",
    )
    .bind(remote.media_id)
    .fetch_optional(&state.pool)
    .await?;
    if let Some(row) = &row {
        if let Some(target) = place_by_tmdb(state, remote, row).await? {
            return Ok(Some(target));
        }
        if let Some(target) = place_by_tvdb(state, api, token, remote, row).await? {
            return Ok(Some(target));
        }
        // Known to a list but not placeable from it: a name could only guess.
        if row.0.is_some() || row.3.is_some() || row.5.is_some() {
            return Ok(None);
        }
    }
    place_by_name(state, remote).await
}

/// Fribb's anime-lists names the TMDB entry, and for a show TMDB's own season
/// and offset, so nothing has to be worked out. Used only when no other
/// AniList entry claims the same spot, which is what an unlisted second cour
/// looks like.
async fn place_by_tmdb(
    state: &AppState,
    remote: &Remote,
    row: &MapRow,
) -> AppResult<Option<Target>> {
    let (kind, Some(tmdb_id), season, offset) = (row.4.as_deref(), row.5, row.6, row.7) else {
        return Ok(None);
    };
    let is_film = remote.format.as_deref() == Some("MOVIE");
    match kind {
        Some("movie") => {
            // A twelve-episode series is not a film, whatever a list says.
            let single = is_film || remote.episodes.is_none_or(|n| n <= 1);
            Ok(single.then_some(Target::Movie(tmdb_id)))
        }
        Some("tv") if !is_film => {
            // Specials are TMDB season 0, which Frog does not place entries in.
            let Some(season) = season.filter(|s| *s >= 1) else {
                return Ok(None);
            };
            let (shared,): (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM anime_map
                  WHERE tmdb_kind = 'tv' AND tmdb_id = ? AND tmdb_season = ? AND tmdb_offset IS ?",
            )
            .bind(tmdb_id)
            .bind(season)
            .bind(offset)
            .fetch_one(&state.pool)
            .await?;
            if shared > 1 {
                return Ok(None);
            }
            let detail = state.tmdb.tv(tmdb_id).await?;
            let Some(count) = models::seasons(&detail)
                .into_iter()
                .find(|s| s.season_number == season)
                .map(|s| s.episode_count)
            else {
                return Ok(None);
            };
            let offset = offset.unwrap_or(0);
            // A season TMDB has not filled in yet still exists; one too short
            // to hold this entry means the list and TMDB disagree.
            if count > 0 && offset >= count {
                return Ok(None);
            }
            Ok(Some(Target::Show {
                tmdb_id,
                season,
                offset,
            }))
        }
        _ => Ok(None),
    }
}

/// Anime-IDs gives TVDB's season and offset, turned into TMDB's numbering
/// through the lengths of the seasons before it.
async fn place_by_tvdb(
    state: &AppState,
    api: &Api<'_>,
    token: &str,
    remote: &Remote,
    row: &MapRow,
) -> AppResult<Option<Target>> {
    let (tvdb_id, tvdb_season, offset, imdb_id) = (row.0, row.1, row.2, row.3.as_deref());
    let is_film = remote.format.as_deref() == Some("MOVIE");

    if let Some(imdb) = imdb_id {
        if is_film || tvdb_id.is_none() {
            if let Some((kind, id)) = tmdb_by_external(state, imdb, "imdb_id").await? {
                if kind == "movie" {
                    return Ok(Some(Target::Movie(id)));
                }
            }
        }
    }
    if is_film {
        // A film filed under a show's specials: no safe place for it.
        return Ok(None);
    }

    let (Some(tvdb_id), Some(tvdb_season)) = (tvdb_id, tvdb_season) else {
        return Ok(None);
    };
    // When Fribb's list already named the show, TVDB need not be asked.
    let tmdb_id = match (row.4.as_deref(), row.5) {
        (Some("tv"), Some(id)) => id,
        _ => {
            let Some((kind, id)) = tmdb_by_external(state, &tvdb_id.to_string(), "tvdb_id").await?
            else {
                return Ok(None);
            };
            if kind != "tv" {
                return Ok(None);
            }
            id
        }
    };
    let detail = state.tmdb.tv(tmdb_id).await?;
    let seasons: Vec<(i64, i64)> = models::seasons(&detail)
        .into_iter()
        .map(|s| (s.season_number, s.episode_count))
        .collect();

    let earlier = if tvdb_season > 1 {
        earlier_episodes(state, api, token, tvdb_id, tvdb_season).await?
    } else {
        Some(0)
    };
    Ok(
        place_tv(tvdb_season, offset, remote.episodes, &seasons, earlier).map(
            |(season, offset)| Target::Show {
                tmdb_id,
                season,
                offset,
            },
        ),
    )
}

const CHAIN_QUERY: &str = "query ($id: Int) {
  Media(id: $id, type: ANIME) {
    id format episodes seasonYear startDate { year } title { english romaji native }
    relations { edges { relationType node { id type format } } }
  }
}";

/// The formats that make up a series' main run, as opposed to films, OVAs
/// and specials hanging off it.
fn main_run(format: Option<&str>) -> bool {
    matches!(format, Some("TV" | "TV_SHORT" | "ONA"))
}

/// One step of a series as AniList links it.
struct Link {
    prequel: Option<i64>,
    episodes: Option<i64>,
    format: Option<String>,
    year: Option<i64>,
    titles: Vec<String>,
}

/// (prequel, episodes, format, year, titles, fetched_at)
type ChainRow = (
    Option<i64>,
    Option<i64>,
    Option<String>,
    Option<i64>,
    String,
    String,
);

/// What comes before `id`, from the shared cache or AniList. An entry whose
/// length was still unknown is asked about again after a day.
async fn chain_link(
    state: &AppState,
    api: &Api<'_>,
    token: &str,
    id: i64,
) -> AppResult<Option<Link>> {
    let row: Option<ChainRow> = sqlx::query_as(
        "SELECT prequel, episodes, format, year, titles, fetched_at FROM anilist_chain WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    if let Some((prequel, episodes, format, year, titles, at)) = row {
        let recent = chrono::DateTime::parse_from_rfc3339(&at)
            .is_ok_and(|at| Utc::now() - at.with_timezone(&Utc) < chrono::Duration::days(1));
        if episodes.is_some() || recent {
            return Ok(Some(Link {
                prequel,
                episodes,
                format,
                year,
                titles: serde_json::from_str(&titles).unwrap_or_default(),
            }));
        }
    }

    let data = api.graphql(token, CHAIN_QUERY, json!({ "id": id })).await?;
    let media = &data["Media"];
    if media.is_null() {
        return Ok(None);
    }
    // Exactly one main-run prequel; a film or OVA before it does not count,
    // and two would be a guess.
    let prequels: Vec<i64> = media["relations"]["edges"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["relationType"].as_str() == Some("PREQUEL"))
        .filter(|e| e["node"]["type"].as_str() == Some("ANIME"))
        .filter(|e| main_run(e["node"]["format"].as_str()))
        .filter_map(|e| e["node"]["id"].as_i64())
        .collect();
    let mut titles: Vec<String> = Vec::new();
    for key in ["english", "romaji", "native"] {
        if let Some(t) = media["title"][key].as_str().map(str::trim) {
            if !t.is_empty() && !titles.iter().any(|known| known == t) {
                titles.push(t.to_string());
            }
        }
    }
    let link = Link {
        prequel: match prequels[..] {
            [one] => Some(one),
            _ => None,
        },
        episodes: media["episodes"].as_i64().filter(|n| *n > 0),
        format: media["format"].as_str().map(str::to_string),
        year: media["startDate"]["year"]
            .as_i64()
            .or_else(|| media["seasonYear"].as_i64()),
        titles,
    };
    sqlx::query(
        "INSERT INTO anilist_chain (id, prequel, episodes, format, year, titles, fetched_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET
            prequel = excluded.prequel, episodes = excluded.episodes, format = excluded.format,
            year = excluded.year, titles = excluded.titles, fetched_at = excluded.fetched_at",
    )
    .bind(id)
    .bind(link.prequel)
    .bind(link.episodes)
    .bind(link.format.as_deref())
    .bind(link.year)
    .bind(serde_json::to_string(&link.titles)?)
    .bind(Utc::now().to_rfc3339())
    .execute(&state.pool)
    .await?;
    Ok(Some(link))
}

/// A later season nobody lists: walk AniList's prequels back to the first
/// season, place that, and count this one's start on from it through TMDB's
/// seasons. Season 3 after a 25-episode and a 12-episode season starts at
/// episode 38 of the series, wherever TMDB's seasons fall.
async fn place_after_prequels(
    state: &AppState,
    api: &Api<'_>,
    token: &str,
    remote: &Remote,
) -> AppResult<Option<Target>> {
    if !main_run(remote.format.as_deref()) {
        return Ok(None);
    }
    let mut before = 0;
    let mut id = remote.media_id;
    let mut root: Option<(i64, Link)> = None;
    // Long-running series have a dozen seasons at most; a loop in the
    // relations must not run forever.
    for _ in 0..24 {
        let Some(link) = chain_link(state, api, token, id).await? else {
            return Ok(None);
        };
        let Some(prequel) = link.prequel else {
            root = Some((id, link));
            break;
        };
        let Some(earlier) = chain_link(state, api, token, prequel).await? else {
            return Ok(None);
        };
        let Some(length) = earlier.episodes else {
            return Ok(None);
        };
        before += length;
        id = prequel;
    }
    let Some((root_id, link)) = root else {
        return Ok(None);
    };
    if root_id == remote.media_id {
        // Already the first season, and nothing placed it.
        return Ok(None);
    }
    let first = Remote {
        media_id: root_id,
        name: link.titles.first().cloned().unwrap_or_default(),
        titles: link.titles,
        format: link.format,
        episodes: link.episodes,
        year: link.year,
        view: remote.view.clone(),
    };
    let Some(Target::Show {
        tmdb_id,
        season,
        offset,
    }) = place_known(state, api, token, &first).await?
    else {
        return Ok(None);
    };
    let detail = state.tmdb.tv(tmdb_id).await?;
    let mut seasons: Vec<(i64, i64)> = models::seasons(&detail)
        .into_iter()
        .map(|s| (s.season_number, s.episode_count))
        .filter(|(n, _)| *n >= 1)
        .collect();
    seasons.sort_unstable();
    let start: i64 = seasons
        .iter()
        .filter(|(n, _)| *n < season)
        .map(|(_, count)| count)
        .sum::<i64>()
        + offset;
    Ok(
        locate(start + before, &seasons).map(|(season, offset)| Target::Show {
            tmdb_id,
            season,
            offset,
        }),
    )
}

/// Letters and digits only, lower case: "Frieren: Beyond Journey's End" and
/// "FRIEREN - Beyond Journey’s End" are the same name.
pub fn plain(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// For entries neither list knows: one TMDB animation with exactly this name
/// that started the same year, or failing that a year either side (a festival
/// premiere, a late-December first broadcast). A show matched this way is its
/// first season, since a later season starts later than the show does. Two
/// equally good matches, or none, and it waits.
async fn place_by_name(state: &AppState, remote: &Remote) -> AppResult<Option<Target>> {
    let film = match remote.format.as_deref() {
        Some("MOVIE") => true,
        Some("TV" | "TV_SHORT" | "ONA") => false,
        _ => return Ok(None),
    };
    let Some(year) = remote.year else {
        return Ok(None);
    };
    let names: Vec<String> = remote
        .titles
        .iter()
        .map(|t| plain(t))
        .filter(|t| !t.is_empty())
        .collect();

    // (id, started the same year)
    let mut found: Vec<(i64, bool)> = Vec::new();
    for title in &remote.titles {
        let results = state.tmdb.search_multi(title, 1).await?;
        for r in results["results"].as_array().into_iter().flatten() {
            if r["media_type"].as_str() != Some(if film { "movie" } else { "tv" }) {
                continue;
            }
            let animated = r["genre_ids"]
                .as_array()
                .is_some_and(|g| g.iter().any(|id| id.as_i64() == Some(ANIMATION)));
            let date = r["release_date"]
                .as_str()
                .or_else(|| r["first_air_date"].as_str())
                .unwrap_or("");
            let Some(started) = date.get(..4).and_then(|y| y.parse::<i64>().ok()) else {
                continue;
            };
            let named = ["title", "name", "original_title", "original_name"]
                .iter()
                .filter_map(|k| r[*k].as_str())
                .any(|n| names.contains(&plain(n)));
            if let (true, true, Some(id)) = (animated, named, r["id"].as_i64()) {
                if (started - year).abs() <= 1 && !found.iter().any(|(known, _)| *known == id) {
                    found.push((id, started == year));
                }
            }
        }
    }
    let exact: Vec<i64> = found.iter().filter(|f| f.1).map(|f| f.0).collect();
    let near: Vec<i64> = found.iter().map(|f| f.0).collect();
    let tmdb_id = match (&exact[..], &near[..]) {
        ([one], _) => *one,
        ([], [one]) => *one,
        _ => return Ok(None),
    };
    if film {
        return Ok(Some(Target::Movie(tmdb_id)));
    }
    let detail = state.tmdb.tv(tmdb_id).await?;
    let has_first = models::seasons(&detail)
        .into_iter()
        .any(|s| s.season_number == 1);
    Ok(has_first.then_some(Target::Show {
        tmdb_id,
        season: 1,
        offset: 0,
    }))
}

/// How many episodes TVDB seasons 1 .. `season - 1` of this show hold, from
/// the AniList entries that make them up. `None` if any of them is unknown.
async fn earlier_episodes(
    state: &AppState,
    api: &Api<'_>,
    token: &str,
    tvdb_id: i64,
    season: i64,
) -> AppResult<Option<i64>> {
    let rows: Vec<(i64, i64, i64)> = sqlx::query_as(
        "SELECT anilist_id, tvdb_season, tvdb_epoffset FROM anime_map
          WHERE tvdb_id = ? AND tvdb_season BETWEEN 1 AND ?",
    )
    .bind(tvdb_id)
    .bind(season - 1)
    .fetch_all(&state.pool)
    .await?;
    let ids: Vec<i64> = rows.iter().map(|r| r.0).collect();
    let episodes = media_episodes(state, api, token, &ids).await?;

    let mut length: BTreeMap<i64, i64> = BTreeMap::new();
    for (id, s, offset) in rows {
        let Some(Some(n)) = episodes.get(&id) else {
            return Ok(None);
        };
        let end = offset + n;
        let slot = length.entry(s).or_insert(0);
        *slot = (*slot).max(end);
    }
    if (1..season).any(|s| !length.contains_key(&s)) {
        return Ok(None);
    }
    Ok(Some(length.values().sum()))
}

/// What TMDB might mean by this entry, for the person to choose from: found
/// under every title AniList has, animation first, the right year next.
async fn candidates(state: &AppState, remote: &Remote) -> Vec<Value> {
    let mut seen: HashSet<(String, i64)> = HashSet::new();
    let mut ranked: Vec<(i64, usize, Value)> = Vec::new();
    let titles = if remote.titles.is_empty() {
        vec![remote.name.clone()]
    } else {
        remote.titles.clone()
    };
    for title in &titles {
        let Ok(found) = state.tmdb.search_multi(title, 1).await else {
            continue;
        };
        for r in found["results"].as_array().cloned().unwrap_or_default() {
            let Some(kind) = r["media_type"]
                .as_str()
                .filter(|k| *k == "movie" || *k == "tv")
            else {
                continue;
            };
            let Some(id) = r["id"].as_i64() else { continue };
            if !seen.insert((kind.to_string(), id)) {
                continue;
            }
            let date = r["release_date"]
                .as_str()
                .or_else(|| r["first_air_date"].as_str())
                .unwrap_or("");
            let year = date.get(..4).and_then(|y| y.parse::<i64>().ok());
            let animated = r["genre_ids"]
                .as_array()
                .is_some_and(|g| g.iter().any(|g| g.as_i64() == Some(ANIMATION)));
            let rank = i64::from(!animated) * 2 + i64::from(year.is_none() || year != remote.year);
            let order = ranked.len();
            ranked.push((
                rank,
                order,
                json!({
                    "kind": kind,
                    "tmdb_id": id,
                    "name": r["title"].as_str().or_else(|| r["name"].as_str()).unwrap_or(""),
                    "year": date.get(..4),
                    "poster_path": r["poster_path"],
                }),
            ));
        }
    }
    ranked.sort_by_key(|(rank, order, _)| (*rank, *order));
    ranked.into_iter().take(6).map(|(_, _, v)| v).collect()
}

// --- what Frog says about an entry -------------------------------------------

/// The episodes an entry covers, in order: from `offset` into `season`,
/// carrying on into later seasons, `limit` long (or to the season's end when
/// AniList does not know the length yet).
async fn run_ids(
    pool: &SqlitePool,
    title_id: i64,
    season: i64,
    offset: i64,
    episodes: Option<i64>,
) -> AppResult<Vec<(i64, bool)>> {
    // (episode id, aired)
    let rows: Vec<(i64, bool)> = sqlx::query_as(
        "SELECT id, (air_date IS NULL OR air_date <= date('now')) FROM episode
          WHERE title_id = ? AND season_number > 0
            AND (season_number > ? OR (season_number = ? AND episode_number > ?))
            AND (? IS NOT NULL OR season_number = ?)
          ORDER BY season_number, episode_number
          LIMIT ?",
    )
    .bind(title_id)
    .bind(season)
    .bind(season)
    .bind(offset)
    .bind(episodes)
    .bind(season)
    .bind(episodes.unwrap_or(10_000))
    .fetch_all(pool)
    .await?;
    Ok(rows)
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

/// An entry as Frog sees it, or `None` when the title is not in the library.
async fn frog_view(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    target: Target,
    episodes: Option<i64>,
) -> AppResult<Option<View>> {
    let Some(entry) = store::get_entry(pool, user_id, title_id).await? else {
        return Ok(None);
    };
    let score = entry.rating.map(|r| r.round() as i64).unwrap_or(0);
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
                score,
            }
        }
        Target::Show { season, offset, .. } => {
            let run = run_ids(pool, title_id, season, offset, episodes).await?;
            let ids: Vec<i64> = run.iter().map(|r| r.0).collect();
            let progress = watched_among(pool, user_id, &ids).await?;
            let finished = episodes.is_some_and(|n| n > 0 && progress >= n);
            let status = match entry.status.as_str() {
                "paused" | "dropped" if !finished => frog_to_anilist(&entry.status),
                _ if finished => "COMPLETED",
                _ if progress > 0 => "CURRENT",
                "completed" => "COMPLETED",
                _ => "PLANNING",
            };
            View {
                status: status.into(),
                progress,
                score,
            }
        }
    };
    Ok(Some(view))
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

    let rating = if a.score > 0 {
        match mode {
            Mode::Merge => Some(
                existing
                    .as_ref()
                    .and_then(|e| e.rating)
                    .or(Some(a.score as f64)),
            ),
            Mode::Follow => Some(Some(a.score as f64)),
        }
    } else if mode == Mode::Follow && before.is_some_and(|b| b.score > 0) {
        // The score was taken off on AniList.
        Some(None)
    } else {
        None
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
        rating,
        ..Default::default()
    };
    store::apply_patch(&state.pool, user_id, title_id, &patch).await?;

    match target {
        Target::Movie(_) => {
            if a.status == "COMPLETED" {
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
                .execute(&state.pool)
                .await?;
            }
        }
        Target::Show { season, offset, .. } => {
            ensure_episodes(state, title_id, &detail).await?;
            let run = run_ids(&state.pool, title_id, season, offset, remote.episodes).await?;
            let upto = a.progress.max(0) as usize;
            let tick: Vec<i64> = run
                .iter()
                .take(upto)
                .filter(|(_, aired)| *aired)
                .map(|(id, _)| *id)
                .collect();
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
                // Progress went down on AniList: take those ticks back.
                for (id, _) in run.iter().skip(upto) {
                    sqlx::query("DELETE FROM watch_event WHERE user_id = ? AND episode_id = ?")
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

/// A show's one status, from the AniList entries that make it up: the latest
/// season you have started says where you are.
async fn settle_show(
    state: &AppState,
    user_id: i64,
    title_id: i64,
    entries: &[(i64, i64, String)],
    mode: Mode,
) -> AppResult<()> {
    // (season, offset, AniList status), in viewing order.
    let mut ordered = entries.to_vec();
    ordered.sort_by_key(|(s, o, _)| (*s, *o));
    let last_started = ordered.iter().rposition(|(_, _, st)| st != "PLANNING");
    let desired = match last_started {
        None => "planned",
        Some(i) => match ordered[i].2.as_str() {
            "COMPLETED" if i + 1 == ordered.len() => "completed",
            "COMPLETED" | "CURRENT" | "REPEATING" => "watching",
            "PAUSED" => "paused",
            "DROPPED" => "dropped",
            _ => "watching",
        },
    };
    let Some(entry) = store::get_entry(&state.pool, user_id, title_id).await? else {
        return Ok(());
    };
    let status = match mode {
        Mode::Merge => merged_status(&entry.status, desired),
        Mode::Follow => desired.to_string(),
    };
    if status != entry.status {
        let patch = LibraryPatch {
            status: Some(status),
            ..Default::default()
        };
        store::apply_patch(&state.pool, user_id, title_id, &patch).await?;
    }
    Ok(())
}

// --- one sync -----------------------------------------------------------------

#[derive(Debug, Default, Serialize)]
pub struct Summary {
    pub to_frog: i64,
    pub to_anilist: i64,
    pub unsure: i64,
    pub entries: i64,
}

/// (media_id, state, title_id, kind, tmdb_id, season, offset,
///  al_status, al_progress, al_score, frog_status, frog_progress, frog_score, detached)
type EntryRow = (
    i64,
    String,
    Option<i64>,
    Option<String>,
    Option<i64>,
    Option<i64>,
    i64,
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<String>,
    Option<i64>,
    Option<i64>,
    bool,
);

struct Known {
    state: String,
    title_id: Option<i64>,
    target: Option<Target>,
    al: Option<View>,
    frog: Option<View>,
    detached: bool,
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
                a.frog_status, a.frog_progress, a.frog_score, a.detached
           FROM anilist_entry a LEFT JOIN title t ON t.id = a.title_id
          WHERE a.user_id = ?",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let mut out = HashMap::new();
    for r in rows {
        let target = match (r.3.as_deref(), r.4) {
            (Some("movie"), Some(id)) => Some(Target::Movie(id)),
            (Some("tv"), Some(id)) => r.5.map(|season| Target::Show {
                tmdb_id: id,
                season,
                offset: r.6,
            }),
            _ => None,
        };
        out.insert(
            r.0,
            Known {
                state: r.1,
                title_id: r.2,
                target,
                al: snapshot(r.7, r.8, r.9),
                frog: snapshot(r.10, r.11, r.12),
                detached: r.13,
            },
        );
    }
    Ok(out)
}

async fn save_entry(
    pool: &SqlitePool,
    user_id: i64,
    remote: &Remote,
    state_name: &str,
    placed: Option<(i64, Target)>,
    candidates: Option<&[Value]>,
) -> AppResult<()> {
    let (title_id, season, offset) = match placed {
        Some((id, Target::Show { season, offset, .. })) => (Some(id), Some(season), offset),
        Some((id, Target::Movie(_))) => (Some(id), None, 0),
        None => (None, None, 0),
    };
    sqlx::query(
        "INSERT INTO anilist_entry (user_id, media_id, name, format, episodes, state, title_id,
                                    season, episode_offset, candidates_json, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (user_id, media_id) DO UPDATE SET
            name = excluded.name, format = excluded.format, episodes = excluded.episodes,
            state = excluded.state, title_id = excluded.title_id, season = excluded.season,
            episode_offset = excluded.episode_offset,
            candidates_json = CASE WHEN excluded.candidates_json = '[]'
                                   THEN anilist_entry.candidates_json
                                   ELSE excluded.candidates_json END,
            updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(remote.media_id)
    .bind(&remote.name)
    .bind(&remote.format)
    .bind(remote.episodes)
    .bind(state_name)
    .bind(title_id)
    .bind(season)
    .bind(offset)
    .bind(serde_json::to_string(&candidates.unwrap_or(&[]))?)
    .bind(Utc::now().to_rfc3339())
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

/// Sync one account, start to finish. Records the outcome on the link.
pub async fn sync_user(state: &AppState, user_id: i64) -> AppResult<Summary> {
    let Some(_running) = RunningGuard::claim(user_id) else {
        return Ok(Summary::default());
    };
    let result = sync_inner(state, user_id).await;
    let now = Utc::now().to_rfc3339();
    match &result {
        Ok(summary) => {
            sqlx::query(
                "UPDATE anilist_link SET synced_at = ?, dirty = 0, last_error = NULL,
                        last_summary = ? WHERE user_id = ?",
            )
            .bind(&now)
            .bind(serde_json::to_string(summary)?)
            .bind(user_id)
            .execute(&state.pool)
            .await?;
        }
        Err(e) => {
            let message = match e {
                AppError::Forbidden(m) | AppError::BadRequest(m) => m.clone(),
                AppError::Tmdb(_) | AppError::TmdbUnconfigured => {
                    "Could not reach TMDB to place your titles. Frog will try again shortly."
                        .to_string()
                }
                _ => "Could not reach AniList. Frog will try again shortly.".to_string(),
            };
            tracing::warn!(user_id, error = %e, "AniList sync failed");
            sqlx::query("UPDATE anilist_link SET synced_at = ?, last_error = ? WHERE user_id = ?")
                .bind(&now)
                .bind(message)
                .bind(user_id)
                .execute(&state.pool)
                .await?;
        }
    }
    result
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

    let data = api
        .graphql(&token, LIST_QUERY, json!({ "userId": anilist_user_id }))
        .await?;
    let remotes = parse_list(&data);
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

    let mut summary = Summary {
        entries: remotes.len() as i64,
        ..Default::default()
    };
    // Shows changed from AniList's side, and how firmly.
    let mut shows: HashMap<i64, Mode> = HashMap::new();

    for remote in &remotes {
        let prior = known.get(&remote.media_id);
        if prior.is_some_and(|k| k.state == "ignored" || k.detached) {
            continue;
        }

        // Where it goes: a placement already made (or chosen) stays.
        let target = match prior.and_then(|k| k.target.filter(|_| k.state == "placed")) {
            Some(t) => Some(t),
            None => place(state, &api, &token, remote).await?,
        };
        let Some(target) = target else {
            summary.unsure += 1;
            if prior.is_none_or(|k| k.state != "unsure") {
                let options = candidates(state, remote).await;
                save_entry(&state.pool, user_id, remote, "unsure", None, Some(&options)).await?;
            }
            continue;
        };

        let (kind, tmdb_id) = match target {
            Target::Movie(id) => ("movie", id),
            Target::Show { tmdb_id, .. } => ("tv", tmdb_id),
        };
        let title_id = match prior.and_then(|k| k.title_id) {
            Some(id) => id,
            None => store::ensure_title(state, kind, tmdb_id).await?.0,
        };
        save_entry(
            &state.pool,
            user_id,
            remote,
            "placed",
            Some((title_id, target)),
            None,
        )
        .await?;

        let snap_al = prior.and_then(|k| k.al.clone());
        let snap_frog = prior.and_then(|k| k.frog.clone());
        let a = &remote.view;
        let f = frog_view(&state.pool, user_id, title_id, target, remote.episodes).await?;

        // Removed from Frog after it was synced: leave it be from now on.
        if f.is_none() && snap_al.is_some() {
            save_snapshots(&state.pool, user_id, remote.media_id, a, None, true).await?;
            continue;
        }

        let first = snap_al.is_none();
        let al_changed = !first && snap_al.as_ref() != Some(a);
        let frog_changed = !first && f.as_ref() != snap_frog.as_ref();
        let frog_progress = f.as_ref().map(|f| f.progress).unwrap_or(0);
        // Both sides moved: whoever is further along wins.
        let follow = al_changed && (!frog_changed || a.progress >= frog_progress);

        let mode = if first {
            Some(Mode::Merge)
        } else if follow {
            Some(Mode::Follow)
        } else {
            None
        };
        if let Some(mode) = mode {
            let created = apply(state, user_id, remote, target, mode, snap_al.as_ref()).await?;
            if kind == "tv" {
                // New to the library, or changed on AniList: AniList decides.
                let firm = if created { Mode::Follow } else { mode };
                let slot = shows.entry(title_id).or_insert(firm);
                if firm == Mode::Follow {
                    *slot = Mode::Follow;
                }
            }
        }

        let f_now = frog_view(&state.pool, user_id, title_id, target, remote.episodes).await?;
        if f_now != f {
            summary.to_frog += 1;
        }

        // Frog has something AniList does not: send it.
        let mut final_al = a.clone();
        if let Some(f_now) = f_now.as_ref() {
            let send = if first {
                // A first sync only sends episodes watched in Frog that
                // AniList has not seen. Frog keeps one score per show and
                // AniList one per season, so scores wait for a real change.
                (f_now.progress > a.progress).then(|| View {
                    status: f_now.status.clone(),
                    progress: f_now.progress,
                    score: a.score,
                })
            } else if frog_changed && !follow {
                // Only what changed in Frog; the rest stays as AniList has it.
                // A tick should not also send the show's score to a season.
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

    // One status per show, once all of its seasons are in.
    for (title_id, mode) in shows {
        let rows: Vec<(i64, i64, String, i64)> = sqlx::query_as(
            "SELECT COALESCE(season, 0), episode_offset, COALESCE(al_status, 'PLANNING'), media_id
               FROM anilist_entry
              WHERE user_id = ? AND title_id = ? AND state = 'placed' AND detached = 0",
        )
        .bind(user_id)
        .bind(title_id)
        .fetch_all(&state.pool)
        .await?;
        let entries: Vec<(i64, i64, String)> =
            rows.iter().map(|r| (r.0, r.1, r.2.clone())).collect();
        settle_show(state, user_id, title_id, &entries, mode).await?;
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
            let f = frog_view(&state.pool, user_id, title_id, target, remote.episodes).await?;
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
/// enough to look again. Expired tokens are skipped; Settings asks for a
/// reconnect instead.
pub async fn due(pool: &SqlitePool) -> AppResult<Vec<i64>> {
    let idle = (Utc::now() - chrono::Duration::minutes(IDLE_SYNC_MINUTES)).to_rfc3339();
    let settle = (Utc::now() - chrono::Duration::seconds(45)).to_rfc3339();
    let now = Utc::now().to_rfc3339();
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT user_id FROM anilist_link
          WHERE expires_at > ?
            AND (synced_at IS NULL OR synced_at < ? OR (dirty = 1 AND synced_at < ?))",
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

    // Jujutsu Kaisen as TMDB has it: one long season.
    const JJK: [(i64, i64); 1] = [(1, 47)];

    #[test]
    fn a_later_tracker_season_lands_inside_one_long_tmdb_season() {
        // TVDB season 2 does not exist on TMDB; the 24 episodes before it put
        // its first episode at TMDB S1E25.
        assert_eq!(place_tv(2, 0, Some(23), &JJK, Some(24)), Some((1, 24)));
        // Season 1 is where it always was.
        assert_eq!(place_tv(1, 0, Some(24), &JJK, Some(0)), Some((1, 0)));
    }

    #[test]
    fn matching_numbering_is_kept_and_a_split_cour_keeps_its_offset() {
        let aot = [(1, 25), (2, 12), (3, 22), (4, 30)];
        assert_eq!(place_tv(3, 12, Some(10), &aot, None), Some((3, 12)));
        assert_eq!(place_tv(4, 0, Some(16), &aot, None), Some((4, 0)));
    }

    #[test]
    fn absolute_order_walks_tmdb_seasons() {
        let seasons = [(0, 5), (1, 61), (2, 16), (3, 14)];
        // Specials never count towards the position.
        assert_eq!(place_tv(-1, 70, None, &seasons, None), Some((2, 9)));
        assert_eq!(place_tv(-1, 0, None, &seasons, None), Some((1, 0)));
        assert_eq!(locate(1000, &seasons), None);
    }

    #[test]
    fn an_unknown_earlier_season_means_unsure_not_a_guess() {
        assert_eq!(place_tv(2, 0, Some(12), &JJK, None), None);
        // Specials are never placed automatically.
        assert_eq!(place_tv(0, 0, Some(1), &JJK, Some(0)), None);
    }

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
