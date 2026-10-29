//! Everything that reads or writes the user's own data.
//!
//! Routes stay thin: they parse input, call in here, and serialise the result.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, SqlitePool};

use crate::error::{AppError, AppResult};
use crate::palette::{self, Palette};
use crate::state::AppState;
use crate::tmdb::models::{self, EpisodeInfo, TitleCore};

pub const STATUSES: [&str; 5] = ["watching", "completed", "planned", "paused", "dropped"];

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct TitleRow {
    pub id: i64,
    pub kind: String,
    pub tmdb_id: i64,
    pub name: String,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub logo_path: Option<String>,
    pub release_date: Option<String>,
    pub last_air_date: Option<String>,
    pub tmdb_status: Option<String>,
    pub runtime: Option<i64>,
    pub episode_count: Option<i64>,
    pub season_count: Option<i64>,
    pub genres_json: String,
    pub tmdb_vote: Option<f64>,
    pub imdb_id: Option<String>,
    pub collection_id: Option<i64>,
    pub accent_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct LibraryEntry {
    pub status: String,
    pub rating: Option<f64>,
    pub favourite: bool,
    pub notes: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub rewatch_count: i64,
    pub added_at: String,
    pub updated_at: String,
    /// New-episode notifications for this show.
    pub notify: bool,
}

/// A title as the grid and the rows draw it: metadata, my state, my progress.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryItem {
    #[serde(flatten)]
    pub title: TitleRow,
    pub genres: Vec<String>,
    pub accent: Palette,
    pub entry: Option<LibraryEntry>,
    pub watched_episodes: i64,
    pub total_episodes: i64,
    pub last_watched_at: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LibraryPatch {
    pub status: Option<String>,
    /// 0..100 internally; the UI converts to whichever score format is set.
    pub rating: Option<Option<f64>>,
    pub favourite: Option<bool>,
    pub notes: Option<Option<String>>,
    pub notify: Option<bool>,
}

pub fn parse_accent(raw: Option<&str>) -> Palette {
    raw.and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default()
}

pub fn parse_genres(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

pub async fn find_title_id(pool: &SqlitePool, kind: &str, tmdb_id: i64) -> AppResult<Option<i64>> {
    let row: Option<(i64,)> = sqlx::query_as("SELECT id FROM title WHERE kind = ? AND tmdb_id = ?")
        .bind(kind)
        .bind(tmdb_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.0))
}

pub async fn get_title(pool: &SqlitePool, id: i64) -> AppResult<TitleRow> {
    sqlx::query_as::<_, TitleRow>(TITLE_SELECT)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("no such title".into()))
}

const TITLE_SELECT: &str = "SELECT id, kind, tmdb_id, name, overview, poster_path, backdrop_path,
        logo_path, release_date, last_air_date, tmdb_status, runtime, episode_count,
        season_count, genres_json, tmdb_vote, imdb_id, collection_id, accent_json
   FROM title WHERE id = ?";

pub async fn upsert_title(pool: &SqlitePool, core: &TitleCore) -> AppResult<i64> {
    sqlx::query(
        "INSERT INTO title (kind, tmdb_id, name, original_name, overview, poster_path,
                            backdrop_path, logo_path, release_date, last_air_date, tmdb_status,
                            runtime, episode_count, season_count, genres_json, tmdb_vote,
                            imdb_id, collection_id, refreshed_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (kind, tmdb_id) DO UPDATE SET
            name = excluded.name,
            original_name = excluded.original_name,
            overview = excluded.overview,
            poster_path = excluded.poster_path,
            backdrop_path = excluded.backdrop_path,
            logo_path = COALESCE(excluded.logo_path, title.logo_path),
            release_date = excluded.release_date,
            last_air_date = excluded.last_air_date,
            tmdb_status = excluded.tmdb_status,
            runtime = excluded.runtime,
            episode_count = excluded.episode_count,
            season_count = excluded.season_count,
            genres_json = excluded.genres_json,
            tmdb_vote = excluded.tmdb_vote,
            imdb_id = COALESCE(excluded.imdb_id, title.imdb_id),
            collection_id = excluded.collection_id,
            refreshed_at = excluded.refreshed_at",
    )
    .bind(&core.kind)
    .bind(core.tmdb_id)
    .bind(&core.name)
    .bind(&core.original_name)
    .bind(&core.overview)
    .bind(&core.poster_path)
    .bind(&core.backdrop_path)
    .bind(&core.logo_path)
    .bind(&core.release_date)
    .bind(&core.last_air_date)
    .bind(&core.tmdb_status)
    .bind(core.runtime)
    .bind(core.episode_count)
    .bind(core.season_count)
    .bind(&core.genres_json)
    .bind(core.tmdb_vote)
    .bind(&core.imdb_id)
    .bind(core.collection_id)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;

    find_title_id(pool, &core.kind, core.tmdb_id)
        .await?
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("title vanished after upsert")))
}

/// Fetch the full TMDB detail, keep a denormalised copy, and make sure we have
/// an accent palette for it. Returns the local row id plus the raw detail so
/// callers can pull cast, videos and providers out of the same payload.
pub async fn ensure_title(state: &AppState, kind: &str, tmdb_id: i64) -> AppResult<(i64, Value)> {
    if is_local(tmdb_id) {
        return local_detail(state, kind, tmdb_id).await;
    }
    let detail = match kind {
        "movie" => state.tmdb.movie(tmdb_id).await?,
        "tv" => state.tmdb.tv(tmdb_id).await?,
        other => return Err(AppError::BadRequest(format!("unknown kind {other}"))),
    };

    let core = TitleCore::from_detail(kind, &detail)
        .ok_or_else(|| AppError::Tmdb("TMDB returned a title with no id or name".into()))?;
    let id = keep_detail(state, &core, &detail).await?;
    Ok((id, detail))
}

/// Store a title's detail, from TMDB or shaped like TMDB's: the row, what
/// comes next, the page tint and the credits Stats ranks.
async fn keep_detail(state: &AppState, core: &TitleCore, detail: &Value) -> AppResult<i64> {
    let poster = core.poster_path.clone();
    let id = upsert_title(&state.pool, core).await?;
    // What TMDB says comes next: a show between two episodes of one season is
    // airing, one waiting for an episode 1 is between seasons.
    let next = &detail["next_episode_to_air"];
    sqlx::query(
        "UPDATE title SET next_air_date = ?, next_air_season = ?, next_air_episode = ? WHERE id = ?",
    )
    .bind(next["air_date"].as_str().filter(|d| !d.is_empty()))
    .bind(next["season_number"].as_i64())
    .bind(next["episode_number"].as_i64())
    .bind(id)
    .execute(&state.pool)
    .await?;

    ensure_palette(state, id, poster.as_deref()).await;
    // Stats are a nice-to-have on top of the title; never fail the page for them.
    if let Err(e) = store_credits(&state.pool, id, detail).await {
        tracing::warn!(title_id = id, error = %e, "could not store credits");
    }
    Ok(id)
}

/// A title TMDB does not have: its tmdb_id is the AniList id it came from,
/// made negative (see the `anilist_resilience` migration).
pub fn is_local(tmdb_id: i64) -> bool {
    tmdb_id < 0
}

/// A title TMDB does not have, from AniList, in TMDB's shape
/// (`anilist::media`): stored and returned like any other, episodes and all.
/// AniList out of reach, the page still has what Frog kept last time.
async fn local_detail(state: &AppState, kind: &str, tmdb_id: i64) -> AppResult<(i64, Value)> {
    let pool = &state.pool;
    let existing = find_title_id(pool, kind, tmdb_id).await?;
    let media = match crate::anilist::media::fetch(state, -tmdb_id).await {
        Ok(media) => media,
        Err(e) => {
            let Some(id) = existing else { return Err(e) };
            tracing::debug!(tmdb_id, error = %e, "AniList detail unavailable; using what is kept");
            let row = get_title(pool, id).await?;
            return Ok((
                id,
                serde_json::json!({ "id": tmdb_id, "name": row.name, "overview": row.overview }),
            ));
        }
    };
    // A film is a film: an address with the other kind is not this title.
    let film = media["format"].as_str() == Some("MOVIE");
    if existing.is_none() && film != (kind == "movie") {
        return Err(AppError::NotFound("no such title".into()));
    }
    let detail = crate::anilist::media::as_tmdb(state, kind, &media).await?;
    let core = TitleCore::from_detail(kind, &detail)
        .ok_or_else(|| AppError::AniList("AniList returned a title with no name".into()))?;
    let id = keep_detail(state, &core, &detail).await?;
    sqlx::query(
        "UPDATE title SET anilist_id = ?
          WHERE id = ? AND anilist_id IS NULL
            AND NOT EXISTS (SELECT 1 FROM title o WHERE o.anilist_id = ?)",
    )
    .bind(-tmdb_id)
    .bind(id)
    .bind(-tmdb_id)
    .execute(pool)
    .await?;
    if kind == "tv" {
        upsert_episodes(pool, id, &crate::anilist::media::episodes(&media)).await?;
    }
    Ok((id, detail))
}

/// What AniList says about an entry TMDB does not have.
pub struct LocalTitle<'a> {
    pub anilist_id: i64,
    pub kind: &'a str,
    pub name: &'a str,
    pub original_name: Option<&'a str>,
    pub release_date: Option<&'a str>,
    pub episodes: Option<i64>,
    /// AniList's release state, in the words TMDB uses for its own.
    pub status: Option<&'a str>,
}

/// The title an AniList entry is kept as while TMDB has nothing for it.
/// One per AniList id, shared like any title; refreshed from AniList each
/// time, and brought back if it had been folded into a TMDB title before.
pub async fn ensure_local_title(pool: &SqlitePool, t: &LocalTitle<'_>) -> AppResult<i64> {
    let existing: Option<(i64,)> = sqlx::query_as("SELECT id FROM title WHERE anilist_id = ?")
        .bind(t.anilist_id)
        .fetch_optional(pool)
        .await?;
    let now = Utc::now().to_rfc3339();
    if let Some((id,)) = existing {
        sqlx::query(
            "UPDATE title SET name = ?, release_date = COALESCE(?, release_date),
                    episode_count = ?, tmdb_status = ?, merged_into = NULL
              WHERE id = ? AND tmdb_id < 0",
        )
        .bind(t.name)
        .bind(t.release_date)
        .bind(t.episodes)
        .bind(t.status)
        .bind(id)
        .execute(pool)
        .await?;
        return Ok(id);
    }
    let result = sqlx::query(
        "INSERT INTO title (kind, tmdb_id, name, original_name, release_date, episode_count,
                            season_count, tmdb_status, genres_json, anilist_id, refreshed_at,
                            people_synced_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, '[]', ?, NULL, ?)",
    )
    .bind(t.kind)
    .bind(-t.anilist_id)
    .bind(t.name)
    .bind(t.original_name)
    .bind(t.release_date)
    .bind(t.episodes)
    .bind((t.kind == "tv").then_some(1))
    .bind(t.status)
    .bind(t.anilist_id)
    // Credits come with AniList's detail; the TMDB backfill leaves it be.
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

/// (status, rating, favourite, notes, started, finished, rewatches, added, notify, progress)
type MergeEntryRow = (
    String,
    Option<f64>,
    bool,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    String,
    bool,
    Option<i64>,
);

/// Move one account's data from one title to another: the library entry,
/// shelves and film watches. Used when an AniList-only title turns out to be
/// a TMDB one, or a person says where it belongs.
///
/// Nothing is dropped on a clash: the further-along status wins, the rating
/// and dates already on the target stay, notes are kept side by side, and a
/// film watch AniList made up is not copied onto a film that has a watch.
/// What both sides held before is written to `title_merge`. All of it is one
/// transaction, so a failure leaves both titles as they were, and running it
/// again finds nothing left to move. Says whether anything moved.
pub async fn move_title_data(
    pool: &SqlitePool,
    user_id: i64,
    from: i64,
    to: i64,
    reason: &str,
) -> AppResult<bool> {
    if from == to {
        return Ok(false);
    }
    let mut tx = pool.begin().await?;
    let kinds: Vec<(i64, String)> = sqlx::query_as("SELECT id, kind FROM title WHERE id IN (?, ?)")
        .bind(from)
        .bind(to)
        .fetch_all(&mut *tx)
        .await?;
    let kind_of = |id: i64| kinds.iter().find(|k| k.0 == id).map(|k| k.1.clone());
    let (Some(from_kind), Some(to_kind)) = (kind_of(from), kind_of(to)) else {
        return Err(AppError::NotFound("no such title".into()));
    };

    let entry_sql = "SELECT status, rating, favourite, notes, started_at, finished_at,
                            rewatch_count, added_at, notify, progress
                       FROM library_entry WHERE user_id = ? AND title_id = ?";
    let old: Option<MergeEntryRow> = sqlx::query_as(entry_sql)
        .bind(user_id)
        .bind(from)
        .fetch_optional(&mut *tx)
        .await?;
    let have: Option<MergeEntryRow> = sqlx::query_as(entry_sql)
        .bind(user_id)
        .bind(to)
        .fetch_optional(&mut *tx)
        .await?;
    let (films,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM watch_event WHERE user_id = ? AND title_id = ? AND episode_id IS NULL",
    )
    .bind(user_id)
    .bind(from)
    .fetch_one(&mut *tx)
    .await?;
    let (shelved,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM shelf_item i JOIN shelf s ON s.id = i.shelf_id
          WHERE s.user_id = ? AND i.title_id = ?",
    )
    .bind(user_id)
    .bind(from)
    .fetch_one(&mut *tx)
    .await?;
    if old.is_none() && films == 0 && shelved == 0 {
        return Ok(false);
    }

    let as_json = |e: &Option<MergeEntryRow>| {
        e.as_ref().map(|e| {
            serde_json::json!({
                "status": e.0, "rating": e.1, "favourite": e.2, "notes": e.3,
                "started_at": e.4, "finished_at": e.5, "rewatch_count": e.6,
                "added_at": e.7, "notify": e.8, "progress": e.9,
            })
        })
    };
    let before = serde_json::json!({
        "from": as_json(&old),
        "to": as_json(&have),
        "film_watches": films,
        "shelved": shelved,
    });

    if let Some(o) = &old {
        let now = Utc::now().to_rfc3339();
        let merged = match &have {
            None => o.clone(),
            Some(h) => {
                let notes = match (&h.3, &o.3) {
                    (Some(a), Some(b)) if a.trim() != b.trim() => Some(format!("{a}\n\n{b}")),
                    (a, b) => a.clone().or_else(|| b.clone()),
                };
                (
                    crate::routes::transfer::merged_status(&h.0, &o.0),
                    h.1.or(o.1),
                    h.2 || o.2,
                    notes,
                    min_opt(&h.4, &o.4),
                    max_opt(&h.5, &o.5),
                    h.6.max(o.6),
                    h.7.clone().min(o.7.clone()),
                    h.8,
                    h.9,
                )
            }
        };
        // A film's rating is the film's. A show's own rating is not what one
        // AniList entry was scored (that lands on its season when the entry
        // is synced there), so it stays in the record rather than moving.
        let rating = if to_kind == "tv" {
            have.as_ref().and_then(|h| h.1)
        } else {
            merged.1
        };
        sqlx::query(
            "INSERT INTO library_entry (user_id, title_id, status, rating, favourite, notes,
                                        started_at, finished_at, rewatch_count, added_at,
                                        updated_at, notify)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (user_id, title_id) DO UPDATE SET
                status = excluded.status, rating = excluded.rating,
                favourite = excluded.favourite, notes = excluded.notes,
                started_at = excluded.started_at, finished_at = excluded.finished_at,
                rewatch_count = excluded.rewatch_count, added_at = excluded.added_at,
                updated_at = excluded.updated_at",
        )
        .bind(user_id)
        .bind(to)
        .bind(&merged.0)
        .bind(rating)
        .bind(merged.2)
        .bind(&merged.3)
        .bind(&merged.4)
        .bind(&merged.5)
        .bind(merged.6)
        .bind(&merged.7)
        .bind(&now)
        .bind(merged.8)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM library_entry WHERE user_id = ? AND title_id = ?")
            .bind(user_id)
            .bind(from)
            .execute(&mut *tx)
            .await?;
    }

    if from_kind == "movie" && to_kind == "movie" {
        // Made-up watches only go across when the film has none of its own.
        sqlx::query(
            "DELETE FROM watch_event
              WHERE user_id = ? AND title_id = ? AND episode_id IS NULL
                AND source = 'anilist' AND undated = 1
                AND EXISTS (SELECT 1 FROM watch_event w
                             WHERE w.user_id = ? AND w.title_id = ? AND w.episode_id IS NULL)",
        )
        .bind(user_id)
        .bind(from)
        .bind(user_id)
        .bind(to)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE watch_event SET title_id = ?
              WHERE user_id = ? AND title_id = ? AND episode_id IS NULL",
        )
        .bind(to)
        .bind(user_id)
        .bind(from)
        .execute(&mut *tx)
        .await?;
    } else {
        // A film-level watch cannot become a show's: AniList's own made-up one
        // goes (the show's episodes carry it now); any logged by hand stay on
        // the old title, where the diary still has them.
        sqlx::query(
            "DELETE FROM watch_event
              WHERE user_id = ? AND title_id = ? AND episode_id IS NULL
                AND source = 'anilist' AND undated = 1",
        )
        .bind(user_id)
        .bind(from)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        "INSERT OR IGNORE INTO shelf_item (shelf_id, title_id, position)
         SELECT i.shelf_id, ?, i.position FROM shelf_item i JOIN shelf s ON s.id = i.shelf_id
          WHERE s.user_id = ? AND i.title_id = ?",
    )
    .bind(to)
    .bind(user_id)
    .bind(from)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM shelf_item
          WHERE title_id = ? AND shelf_id IN (SELECT id FROM shelf WHERE user_id = ?)",
    )
    .bind(from)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO title_merge (user_id, from_title, to_title, reason, before_json, merged_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(from)
    .bind(to)
    .bind(reason)
    .bind(before.to_string())
    .bind(Utc::now().to_rfc3339())
    .execute(&mut *tx)
    .await?;

    // Once nobody keeps an AniList-only title, it points at where it went,
    // so an old link still finds it.
    sqlx::query(
        "UPDATE title SET merged_into = ?
          WHERE id = ? AND tmdb_id < 0
            AND NOT EXISTS (SELECT 1 FROM library_entry l WHERE l.title_id = title.id)",
    )
    .bind(to)
    .bind(from)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(true)
}

fn min_opt(a: &Option<String>, b: &Option<String>) -> Option<String> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.clone().min(b.clone())),
        (a, b) => a.clone().or_else(|| b.clone()),
    }
}

fn max_opt(a: &Option<String>, b: &Option<String>) -> Option<String> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.clone().max(b.clone())),
        (a, b) => a.clone().or_else(|| b.clone()),
    }
}

/// Replace the credits stats rank by. Marks the title as synced even when TMDB
/// lists nobody, so the worker does not keep asking.
pub async fn store_credits(pool: &SqlitePool, title_id: i64, detail: &Value) -> AppResult<()> {
    let credits = models::stat_credits(detail);
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM title_person WHERE title_id = ?")
        .bind(title_id)
        .execute(&mut *tx)
        .await?;
    for c in &credits {
        sqlx::query(
            "INSERT INTO title_person (title_id, role, tmdb_id, name, profile_path, ord)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(title_id)
        .bind(c.role)
        .bind(c.tmdb_id)
        .bind(&c.name)
        .bind(&c.profile_path)
        .bind(c.ord)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("UPDATE title SET people_synced_at = ? WHERE id = ?")
        .bind(Utc::now().to_rfc3339())
        .bind(title_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Derive the page tint from the poster once, then keep it. A tiny w92 thumb is
/// enough and costs about 5 KB.
async fn ensure_palette(state: &AppState, title_id: i64, poster_path: Option<&str>) {
    let Some(poster) = poster_path else { return };

    let existing: Option<(Option<String>,)> =
        sqlx::query_as("SELECT accent_json FROM title WHERE id = ?")
            .bind(title_id)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();
    if matches!(existing, Some((Some(_),))) {
        return;
    }

    let bytes = match crate::routes::images::cached_image(state, "w92", poster).await {
        Ok((_, b)) => b,
        Err(e) => {
            tracing::debug!(error = %e, "palette: could not load poster");
            return;
        }
    };

    if let Some(p) = palette::extract(&bytes) {
        let json = serde_json::to_string(&p).unwrap_or_default();
        let _ = sqlx::query("UPDATE title SET accent_json = ? WHERE id = ?")
            .bind(json)
            .bind(title_id)
            .execute(&state.pool)
            .await;
    }
}

pub async fn upsert_episodes(
    pool: &SqlitePool,
    title_id: i64,
    episodes: &[EpisodeInfo],
) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    for e in episodes {
        sqlx::query(
            "INSERT INTO episode (title_id, season_number, episode_number, tmdb_episode_id,
                                  name, overview, still_path, air_date, runtime, tmdb_vote)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (title_id, season_number, episode_number) DO UPDATE SET
                tmdb_episode_id = excluded.tmdb_episode_id,
                name = excluded.name,
                overview = excluded.overview,
                still_path = excluded.still_path,
                air_date = excluded.air_date,
                runtime = excluded.runtime,
                tmdb_vote = excluded.tmdb_vote",
        )
        .bind(title_id)
        .bind(e.season_number)
        .bind(e.episode_number)
        .bind(e.tmdb_episode_id)
        .bind(&e.name)
        .bind(&e.overview)
        .bind(&e.still_path)
        .bind(&e.air_date)
        .bind(e.runtime)
        .bind(e.tmdb_vote)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Pull every season of a show into the local episode table. Used when a show
/// is added, so progress and "up next" work without opening each season.
pub async fn sync_all_seasons(state: &AppState, title_id: i64, detail: &Value) -> AppResult<()> {
    let tmdb_id = models::i64_at(detail, "id").unwrap_or_default();
    // A title from AniList has its episodes from AniList, with its detail.
    if is_local(tmdb_id) {
        return Ok(());
    }
    for season in models::seasons(detail) {
        let payload = match state.tmdb.season(tmdb_id, season.season_number).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(season = season.season_number, error = %e, "season sync skipped");
                continue;
            }
        };
        let episodes = models::episodes_of_season(&payload);
        upsert_episodes(&state.pool, title_id, &episodes).await?;
    }
    Ok(())
}

pub async fn get_entry(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
) -> AppResult<Option<LibraryEntry>> {
    let row = sqlx::query_as::<_, LibraryEntry>(
        "SELECT status, rating, favourite, notes, started_at, finished_at,
                rewatch_count, added_at, updated_at, notify
           FROM library_entry WHERE user_id = ? AND title_id = ?",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn apply_patch(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    patch: &LibraryPatch,
) -> AppResult<LibraryEntry> {
    if let Some(status) = &patch.status {
        if !STATUSES.contains(&status.as_str()) {
            return Err(AppError::BadRequest(format!(
                "status must be one of {}",
                STATUSES.join(", ")
            )));
        }
    }
    if let Some(Some(rating)) = patch.rating {
        if !(0.0..=100.0).contains(&rating) {
            return Err(AppError::BadRequest(
                "rating must be between 0 and 100".into(),
            ));
        }
    }

    let now = Utc::now().to_rfc3339();
    let existing = get_entry(pool, user_id, title_id).await?;

    let status = patch
        .status
        .clone()
        .or_else(|| existing.as_ref().map(|e| e.status.clone()))
        .unwrap_or_else(|| "planned".into());

    let rating = match patch.rating {
        Some(v) => v,
        None => existing.as_ref().and_then(|e| e.rating),
    };
    let favourite = patch
        .favourite
        .or_else(|| existing.as_ref().map(|e| e.favourite))
        .unwrap_or(false);
    let notes = match &patch.notes {
        Some(v) => v.clone(),
        None => existing.as_ref().and_then(|e| e.notes.clone()),
    };
    let notify = patch
        .notify
        .or_else(|| existing.as_ref().map(|e| e.notify))
        .unwrap_or(true);

    // Dates follow the status automatically; nobody wants to fill those in.
    let started_at = existing
        .as_ref()
        .and_then(|e| e.started_at.clone())
        .or_else(|| matches!(status.as_str(), "watching" | "completed").then(|| now.clone()));
    let finished_at = match status.as_str() {
        "completed" => existing
            .as_ref()
            .and_then(|e| e.finished_at.clone())
            .or_else(|| Some(now.clone())),
        _ => existing.as_ref().and_then(|e| e.finished_at.clone()),
    };

    sqlx::query(
        "INSERT INTO library_entry (user_id, title_id, status, rating, favourite, notes, started_at,
                                    finished_at, rewatch_count, added_at, updated_at, notify)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, COALESCE((SELECT rewatch_count FROM library_entry WHERE user_id = ? AND title_id = ?), 0), ?, ?, ?)
         ON CONFLICT (user_id, title_id) DO UPDATE SET
            notify = excluded.notify,
            status = excluded.status,
            rating = excluded.rating,
            favourite = excluded.favourite,
            notes = excluded.notes,
            started_at = excluded.started_at,
            finished_at = excluded.finished_at,
            updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(title_id)
    .bind(&status)
    .bind(rating)
    .bind(favourite)
    .bind(&notes)
    .bind(&started_at)
    .bind(&finished_at)
    .bind(user_id)
    .bind(title_id)
    .bind(existing.as_ref().map(|e| e.added_at.clone()).unwrap_or_else(|| now.clone()))
    .bind(&now)
    .bind(notify)
    .execute(pool)
    .await?;

    get_entry(pool, user_id, title_id)
        .await?
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("entry vanished after write")))
}

pub async fn remove_entry(pool: &SqlitePool, user_id: i64, title_id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM library_entry WHERE user_id = ? AND title_id = ?")
        .bind(user_id)
        .bind(title_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, FromRow)]
pub struct Progress {
    pub watched: i64,
    pub total: i64,
}

pub async fn progress(pool: &SqlitePool, user_id: i64, title_id: i64) -> AppResult<Progress> {
    // Specials (season 0) do not count towards finishing a show, and nor do
    // episodes that have not come out. The episode list is shared; only the
    // watches are mine.
    let row: (i64, i64) = sqlx::query_as(concat!(
        "SELECT
            (SELECT COUNT(DISTINCT e.id)
               FROM episode e
               JOIN watch_event w ON w.episode_id = e.id AND w.user_id = ?
              WHERE e.title_id = ? AND e.season_number > 0),
            (SELECT COUNT(*) FROM episode e WHERE e.title_id = ? AND e.season_number > 0 AND ",
        crate::released!(),
        ")"
    ))
    .bind(user_id)
    .bind(title_id)
    .bind(title_id)
    .fetch_one(pool)
    .await?;

    Ok(Progress {
        watched: row.0,
        total: row.1,
    })
}

pub async fn last_watched_at(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
) -> AppResult<Option<String>> {
    // An aggregate always returns exactly one row, and that row holds NULL
    // when nothing matched -- so this has to be Option-inside-the-row, not
    // fetch_optional, or a title with no watches fails to decode.
    let (last,): (Option<String>,) = sqlx::query_as(
        "SELECT MAX(watched_at) FROM watch_event WHERE user_id = ? AND title_id = ?",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_one(pool)
    .await?;
    Ok(last)
}

pub async fn build_item(
    pool: &SqlitePool,
    user_id: i64,
    title: TitleRow,
) -> AppResult<LibraryItem> {
    let entry = get_entry(pool, user_id, title.id).await?;
    let mut p = progress(pool, user_id, title.id).await?;
    if let Some(n) = local_progress(pool, user_id).await?.get(&title.id) {
        p.watched = *n;
    }
    let last = last_watched_at(pool, user_id, title.id).await?;
    let total = if p.total > 0 {
        p.total
    } else {
        title.episode_count.unwrap_or(0)
    };

    Ok(LibraryItem {
        genres: parse_genres(&title.genres_json),
        accent: parse_accent(title.accent_json.as_deref()),
        entry,
        watched_episodes: p.watched,
        total_episodes: total,
        last_watched_at: last,
        title,
    })
}

/// Episodes watched of each title that has no episode list (an AniList-only
/// title AniList lists no episodes for), as a count.
async fn local_progress(
    pool: &SqlitePool,
    user_id: i64,
) -> AppResult<std::collections::HashMap<i64, i64>> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT l.title_id, COALESCE(l.progress, 0)
           FROM library_entry l JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ? AND t.tmdb_id < 0
            AND NOT EXISTS (SELECT 1 FROM episode e
                             WHERE e.title_id = t.id AND e.season_number > 0)",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Set the count of episodes watched of a title with no episode list.
pub async fn set_local_progress(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    progress: i64,
) -> AppResult<()> {
    sqlx::query("UPDATE library_entry SET progress = ? WHERE user_id = ? AND title_id = ?")
        .bind(progress.max(0))
        .bind(user_id)
        .bind(title_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// A library entry with the title it belongs to, for loading them all at once.
#[derive(FromRow)]
struct EntryOfTitle {
    title_id: i64,
    #[sqlx(flatten)]
    entry: LibraryEntry,
}

/// `build_item` for a whole list, in four queries instead of three per title.
///
/// A library of six hundred titles was eighteen hundred queries and over a
/// second before the page could draw. The numbers are the same as
/// `build_item`'s: specials do not count, and the episode list is shared
/// while the watches are this account's.
pub async fn build_items(
    pool: &SqlitePool,
    user_id: i64,
    titles: Vec<TitleRow>,
) -> AppResult<Vec<LibraryItem>> {
    use std::collections::HashMap;

    let entries: HashMap<i64, LibraryEntry> = sqlx::query_as::<_, EntryOfTitle>(
        "SELECT title_id, status, rating, favourite, notes, started_at, finished_at,
                rewatch_count, added_at, updated_at, notify
           FROM library_entry WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| (r.title_id, r.entry))
    .collect();

    let watched: HashMap<i64, i64> = sqlx::query_as::<_, (i64, i64)>(
        "SELECT e.title_id, COUNT(DISTINCT e.id)
           FROM episode e
           JOIN watch_event w ON w.episode_id = e.id AND w.user_id = ?
          WHERE e.season_number > 0
          GROUP BY e.title_id",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();

    let totals: HashMap<i64, i64> = sqlx::query_as::<_, (i64, i64)>(concat!(
        "SELECT e.title_id, COUNT(*)
           FROM episode e
          WHERE e.season_number > 0
            AND e.title_id IN (SELECT title_id FROM library_entry WHERE user_id = ?)
            AND ",
        crate::released!(),
        " GROUP BY e.title_id"
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();

    let last: HashMap<i64, Option<String>> = sqlx::query_as::<_, (i64, Option<String>)>(
        "SELECT title_id, MAX(watched_at) FROM watch_event WHERE user_id = ? GROUP BY title_id",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();
    let counted = local_progress(pool, user_id).await?;

    Ok(titles
        .into_iter()
        .map(|title| {
            let total = totals.get(&title.id).copied().unwrap_or(0);
            let seen = counted.get(&title.id).or_else(|| watched.get(&title.id));
            LibraryItem {
                genres: parse_genres(&title.genres_json),
                accent: parse_accent(title.accent_json.as_deref()),
                entry: entries.get(&title.id).cloned(),
                watched_episodes: seen.copied().unwrap_or(0),
                total_episodes: if total > 0 {
                    total
                } else {
                    title.episode_count.unwrap_or(0)
                },
                last_watched_at: last.get(&title.id).cloned().flatten(),
                title,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_falls_back_to_the_default_theme_colour() {
        assert_eq!(parse_accent(None), Palette::default());
        assert_eq!(parse_accent(Some("not json")), Palette::default());
        let custom = r##"{"accent":"#ff0000","shade":"#100000","tint":"#ffcccc"}"##;
        assert_eq!(parse_accent(Some(custom)).accent, "#ff0000");
    }

    #[test]
    fn genres_survive_a_malformed_column() {
        assert_eq!(
            parse_genres("[\"Drama\",\"Crime\"]"),
            vec!["Drama", "Crime"]
        );
        assert!(parse_genres("").is_empty());
    }
}
