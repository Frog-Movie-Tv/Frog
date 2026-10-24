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
    let detail = match kind {
        "movie" => state.tmdb.movie(tmdb_id).await?,
        "tv" => state.tmdb.tv(tmdb_id).await?,
        other => return Err(AppError::BadRequest(format!("unknown kind {other}"))),
    };

    let core = TitleCore::from_detail(kind, &detail)
        .ok_or_else(|| AppError::Tmdb("TMDB returned a title with no id or name".into()))?;
    let poster = core.poster_path.clone();
    let id = upsert_title(&state.pool, &core).await?;

    ensure_palette(state, id, poster.as_deref()).await;
    Ok((id, detail))
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

pub async fn get_entry(pool: &SqlitePool, title_id: i64) -> AppResult<Option<LibraryEntry>> {
    let row = sqlx::query_as::<_, LibraryEntry>(
        "SELECT status, rating, favourite, notes, started_at, finished_at,
                rewatch_count, added_at, updated_at
           FROM library_entry WHERE title_id = ?",
    )
    .bind(title_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn apply_patch(
    pool: &SqlitePool,
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
    let existing = get_entry(pool, title_id).await?;

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
        "INSERT INTO library_entry (title_id, status, rating, favourite, notes, started_at,
                                    finished_at, rewatch_count, added_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, COALESCE((SELECT rewatch_count FROM library_entry WHERE title_id = ?), 0), ?, ?)
         ON CONFLICT (title_id) DO UPDATE SET
            status = excluded.status,
            rating = excluded.rating,
            favourite = excluded.favourite,
            notes = excluded.notes,
            started_at = excluded.started_at,
            finished_at = excluded.finished_at,
            updated_at = excluded.updated_at",
    )
    .bind(title_id)
    .bind(&status)
    .bind(rating)
    .bind(favourite)
    .bind(&notes)
    .bind(&started_at)
    .bind(&finished_at)
    .bind(title_id)
    .bind(existing.as_ref().map(|e| e.added_at.clone()).unwrap_or_else(|| now.clone()))
    .bind(&now)
    .execute(pool)
    .await?;

    get_entry(pool, title_id)
        .await?
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("entry vanished after write")))
}

pub async fn remove_entry(pool: &SqlitePool, title_id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM library_entry WHERE title_id = ?")
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

pub async fn progress(pool: &SqlitePool, title_id: i64) -> AppResult<Progress> {
    // Specials (season 0) do not count towards finishing a show.
    let row: (i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(DISTINCT e.id)
               FROM episode e
               JOIN watch_event w ON w.episode_id = e.id
              WHERE e.title_id = ? AND e.season_number > 0),
            (SELECT COUNT(*) FROM episode WHERE title_id = ? AND season_number > 0)",
    )
    .bind(title_id)
    .bind(title_id)
    .fetch_one(pool)
    .await?;

    Ok(Progress {
        watched: row.0,
        total: row.1,
    })
}

pub async fn last_watched_at(pool: &SqlitePool, title_id: i64) -> AppResult<Option<String>> {
    // An aggregate always returns exactly one row, and that row holds NULL
    // when nothing matched -- so this has to be Option-inside-the-row, not
    // fetch_optional, or a title with no watches fails to decode.
    let (last,): (Option<String>,) =
        sqlx::query_as("SELECT MAX(watched_at) FROM watch_event WHERE title_id = ?")
            .bind(title_id)
            .fetch_one(pool)
            .await?;
    Ok(last)
}

pub async fn build_item(pool: &SqlitePool, title: TitleRow) -> AppResult<LibraryItem> {
    let entry = get_entry(pool, title.id).await?;
    let p = progress(pool, title.id).await?;
    let last = last_watched_at(pool, title.id).await?;
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
