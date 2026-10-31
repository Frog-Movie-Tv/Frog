//! Writing imported titles into one account's library without trampling
//! what is there. The conflict mode says what happens to a title already
//! tracked; watches are only ever added, never moved or removed, though an
//! undated one from an earlier import takes the day a later import gives.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::catalog::{self, ExternalId};
use crate::error::AppResult;
use crate::state::AppState;
use crate::store::{self, LibraryEntry, LibraryPatch, STATUSES};
use crate::tmdb::models;

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Conflict {
    /// Leave anything already tracked exactly as it is.
    #[default]
    Skip,
    /// Let the file win.
    Overwrite,
    /// Keep in step without undoing anything (AniList sync): add what is new,
    /// move progress and status forward only, and fill a rating only where
    /// there is none. A paused or dropped title stays as you left it.
    Merge,
}

/// How far along a status is, for merging: only a later one replaces it.
fn status_rank(status: &str) -> Option<u8> {
    match status {
        "planned" => Some(0),
        "watching" => Some(1),
        "completed" => Some(2),
        // Paused and dropped are decisions, not progress; nothing overrides them.
        _ => None,
    }
}

/// The status a merge keeps: the incoming one only when it is further along.
/// The AniList sync merges by the same rule.
pub fn merged_status(existing: &str, incoming: &str) -> String {
    match (status_rank(existing), status_rank(incoming)) {
        (Some(have), Some(new)) if new > have => incoming.to_string(),
        _ => existing.to_string(),
    }
}

#[derive(Debug, Deserialize)]
pub struct ApplyItem {
    pub kind: String,
    /// The title as TMDB knows it, or, for an anime TMDB lacks, as AniList
    /// does. Never a Frog id: one from another Frog means nothing here.
    pub tmdb_id: Option<i64>,
    #[serde(default)]
    pub anilist_id: Option<i64>,
    pub status: Option<String>,
    pub rating: Option<f64>,
    pub watched_at: Option<String>,
    #[serde(default)]
    pub favourite: bool,
    #[serde(default)]
    pub rewatch: bool,
    /// For a show: the season this row covers.
    pub season: Option<i64>,
    /// Episodes watched within it. Absent with status "completed" means all.
    pub progress: Option<i64>,
    /// Where this row starts inside the TMDB season. Trackers split a season
    /// into parts ("Season 3 Part 2") that TMDB keeps as one, so part two's
    /// first episode is TMDB's episode 13, not 1.
    #[serde(default)]
    pub episode_offset: i64,
    /// How many episodes this row covers, when the tracker says.
    pub episode_total: Option<i64>,
    /// Exactly these episodes, each on the day it was watched when known.
    /// Takes the place of `season` and `progress`.
    #[serde(default)]
    pub episodes: Vec<EpisodeWatch>,
}

#[derive(Debug, Deserialize)]
pub struct EpisodeWatch {
    pub season: i64,
    pub episode: i64,
    pub watched_at: Option<String>,
}

/// What an import did, as the wizard reports it.
#[derive(Debug, Default, Serialize)]
pub struct Applied {
    pub added: i64,
    pub updated: i64,
    pub skipped: i64,
    /// `{ tmdb_id, error }` for each row that could not be written.
    pub failed: Vec<Value>,
}

/// Write a batch of imported titles into one account's library.
pub async fn apply_items(
    state: &AppState,
    user_id: i64,
    items: Vec<ApplyItem>,
    on_conflict: Conflict,
) -> AppResult<Applied> {
    let mut out = Applied::default();
    for item in items {
        let Some(source) = source_of(&item) else {
            let why = if item.kind == "movie" || item.kind == "tv" {
                "no TMDB or AniList id"
            } else {
                "unknown kind"
            };
            out.failed.push(failure(&item, why));
            continue;
        };
        apply_one(state, user_id, item, &source, on_conflict, &mut out).await?;
    }
    Ok(out)
}

/// Which title a row names: TMDB's id first, else AniList's. A kind that is
/// neither a film nor a show, or no id at all, names nothing.
fn source_of(item: &ApplyItem) -> Option<ExternalId> {
    if item.kind != "movie" && item.kind != "tv" {
        return None;
    }
    match (item.tmdb_id, item.anilist_id) {
        (Some(id), _) if id > 0 => Some(ExternalId::tmdb(&item.kind, id)),
        (_, Some(id)) if id > 0 => Some(ExternalId::AniList(id)),
        _ => None,
    }
}

/// A row that could not be written, as the wizard lists it.
fn failure(item: &ApplyItem, error: &str) -> Value {
    let mut v = json!({ "tmdb_id": item.tmdb_id, "error": error });
    if let Some(id) = item.anilist_id {
        v["anilist_id"] = json!(id);
    }
    v
}

async fn apply_one(
    state: &AppState,
    user_id: i64,
    item: ApplyItem,
    source: &ExternalId,
    on_conflict: Conflict,
    out: &mut Applied,
) -> AppResult<()> {
    let pool = &state.pool;
    // Through merges: a title folded into another is imported onto that one.
    let existing = catalog::find(pool, source).await?;
    let already = match existing {
        Some(id) => store::get_entry(pool, user_id, id).await?.is_some(),
        None => false,
    };
    if already && on_conflict == Conflict::Skip {
        out.skipped += 1;
        return Ok(());
    }

    let (title_id, detail) = match existing {
        // Opened as itself, so the import lands on the surviving title.
        Some(id) => match catalog::load(state, id).await {
            Ok((_, detail)) => (id, detail),
            Err(e) => {
                out.failed.push(failure(&item, &e.to_string()));
                return Ok(());
            }
        },
        None => match catalog::ensure(state, source).await {
            Ok(pair) => pair,
            Err(e) => {
                out.failed.push(failure(&item, &e.to_string()));
                return Ok(());
            }
        },
    };

    // The catalog's word on what it is: an AniList row may name a film as a show.
    let kind = catalog::get_title(pool, title_id).await?.kind;

    let mut status = incoming_status(item.status.as_deref());
    // A later season you only plan to watch must not push a show you are
    // part-way through back onto the watchlist.
    if status == "planned" && store::progress(pool, user_id, title_id).await?.watched > 0 {
        status = "watching".to_string();
    }

    // What a merge compares against, to report only real changes.
    let before = if already && on_conflict == Conflict::Merge {
        let entry = store::get_entry(pool, user_id, title_id).await?;
        let watched = store::progress(pool, user_id, title_id).await?.watched;
        entry.map(|e| (e, watched))
    } else {
        None
    };

    let completed = status == "completed";
    let patch = patch_for(before.as_ref().map(|(e, _)| e), status, &item);
    store::apply_patch(pool, user_id, title_id, &patch).await?;

    if kind == "tv" && !item.episodes.is_empty() {
        tick_episodes(state, user_id, title_id, &detail, &item.episodes).await?;
    } else if kind == "tv" {
        if let Some(season) = item.season {
            if let Some((after, upto)) = season_span(&item, completed) {
                tick_season(state, user_id, title_id, &detail, season, after, upto).await?;
            }
        } else if completed && !already {
            // Finished, with no season given: the whole show was watched,
            // so its episodes count (undated) like any other.
            for s in models::seasons(&detail)
                .into_iter()
                .filter(|s| s.season_number > 0)
            {
                tick_season(
                    state,
                    user_id,
                    title_id,
                    &detail,
                    s.season_number,
                    0,
                    i64::MAX,
                )
                .await?;
            }
        }
    }

    if kind == "movie" {
        film_watches(state, user_id, title_id, &item, completed).await?;
    }

    if kind == "tv" {
        // A row with nothing to tick still has a show behind it. Without its
        // episodes it has no next one, and only TMDB's count to go by.
        if let Err(e) = catalog::ensure_episodes(state, title_id, Some(&detail)).await {
            tracing::warn!(title_id, error = %e, "could not list its episodes");
        }
        // Episodes ticked on a season the review chose: under way, not planned.
        let planned = store::get_entry(pool, user_id, title_id)
            .await?
            .is_some_and(|e| e.status == "planned");
        if planned && store::progress(pool, user_id, title_id).await?.watched > 0 {
            let patch = LibraryPatch {
                status: Some("watching".into()),
                ..Default::default()
            };
            store::apply_patch(pool, user_id, title_id, &patch).await?;
        }
    }

    if let Some((have, watched_before)) = before {
        let now = store::get_entry(pool, user_id, title_id).await?;
        let watched_now = store::progress(pool, user_id, title_id).await?.watched;
        let same = now.is_some_and(|e| {
            e.status == have.status && e.rating == have.rating && e.favourite == have.favourite
        }) && watched_now == watched_before;
        if same {
            out.skipped += 1;
        } else {
            out.updated += 1;
        }
    } else if already {
        out.updated += 1;
    } else {
        out.added += 1;
    }
    Ok(())
}

/// The status a row brings: one Frog knows, or completed.
fn incoming_status(raw: Option<&str>) -> String {
    raw.filter(|s| STATUSES.contains(s))
        .unwrap_or("completed")
        .to_string()
}

/// What a row changes on the library entry. A merge only moves the status
/// forward, fills an empty rating and adds a favourite; otherwise the row's
/// status, rating and favourite are taken as they are, an empty rating
/// included.
fn patch_for(merge_into: Option<&LibraryEntry>, status: String, item: &ApplyItem) -> LibraryPatch {
    match merge_into {
        Some(have) => LibraryPatch {
            status: Some(merged_status(&have.status, &status)),
            rating: Some(have.rating.or(item.rating)),
            favourite: Some(have.favourite || item.favourite),
            notes: None,
            notify: None,
        },
        None => LibraryPatch {
            status: Some(status),
            rating: Some(item.rating),
            favourite: Some(item.favourite),
            notes: None,
            notify: None,
        },
    }
}

/// The episodes of the row's season to tick, as `(after, upto)`: from the
/// row's offset, as many as it watched, or all of them when it is completed
/// and says nothing more. None when there is nothing to tick.
fn season_span(item: &ApplyItem, completed: bool) -> Option<(i64, i64)> {
    let after = item.episode_offset.max(0);
    let count = match (item.progress, completed) {
        (Some(n), _) => Some(n),
        (None, true) => Some(item.episode_total.unwrap_or(i64::MAX - after)),
        (None, false) => None,
    };
    count.filter(|n| *n > 0).map(|n| (after, after + n))
}

/// A film's watches from a row. A finished film with no date still counts as
/// watched: undated, so it is in the totals but not on the day of the import
/// in the diary. A dated row is a diary entry, once. Shows never get a
/// title-level entry: their watches are their episodes, and one here would
/// count the show as a film in the stats.
async fn film_watches(
    state: &AppState,
    user_id: i64,
    title_id: i64,
    item: &ApplyItem,
    completed: bool,
) -> AppResult<()> {
    if completed && item.watched_at.is_none() {
        sqlx::query(
            "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch,
                                      source, undated)
             SELECT ?1, ?2, NULL, ?3, 0, 'import', 1
              WHERE NOT EXISTS (SELECT 1 FROM watch_event
                                 WHERE user_id = ?1 AND title_id = ?2 AND episode_id IS NULL)",
        )
        .bind(user_id)
        .bind(title_id)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&state.pool)
        .await?;
    }

    if let Some(watched_at) = item.watched_at.as_deref() {
        let exists: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM watch_event
              WHERE user_id = ? AND title_id = ? AND episode_id IS NULL
                AND watched_at = ?",
        )
        .bind(user_id)
        .bind(title_id)
        .bind(watched_at)
        .fetch_one(&state.pool)
        .await?;

        if exists.0 == 0 {
            sqlx::query(
                "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at,
                                          is_rewatch, source)
                 VALUES (?, ?, NULL, ?, ?, 'import')",
            )
            .bind(user_id)
            .bind(title_id)
            .bind(watched_at)
            .bind(i64::from(item.rewatch))
            .execute(&state.pool)
            .await?;
        }
    }
    Ok(())
}

/// Mark episodes `after + 1 ..= upto` of a season watched.
///
/// Trackers record how far you got, not when, so these watches are undated:
/// they count towards progress and stats but stay out of the diary rather
/// than all landing on the day of the import.
async fn tick_season(
    state: &AppState,
    user_id: i64,
    title_id: i64,
    detail: &Value,
    season: i64,
    after: i64,
    upto: i64,
) -> AppResult<()> {
    let (have,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM episode WHERE title_id = ? AND season_number = ?")
            .bind(title_id)
            .bind(season)
            .fetch_one(&state.pool)
            .await?;
    if have == 0 {
        catalog::sync_all_seasons(state, title_id, detail).await?;
    }

    sqlx::query(concat!(
        "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch,
                                  source, undated)
         SELECT ?1, ?2, e.id, ?3, 0, 'import', 1
           FROM episode e
          WHERE e.title_id = ?2 AND e.season_number = ?4
            AND e.episode_number > ?5 AND e.episode_number <= ?6
            AND ",
        crate::released!(),
        "
            AND NOT EXISTS (
                  SELECT 1 FROM watch_event w
                   WHERE w.user_id = ?1 AND w.episode_id = e.id)"
    ))
    .bind(user_id)
    .bind(title_id)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(season)
    .bind(after)
    .bind(upto)
    .execute(&state.pool)
    .await?;

    Ok(())
}

/// Mark exactly these episodes watched: dated when the source says when,
/// undated otherwise. An episode already watched is left as it is, unless an
/// earlier import could not say when and this one can.
async fn tick_episodes(
    state: &AppState,
    user_id: i64,
    title_id: i64,
    detail: &Value,
    episodes: &[EpisodeWatch],
) -> AppResult<()> {
    let mut seasons: Vec<i64> = episodes.iter().map(|e| e.season).collect();
    seasons.sort_unstable();
    seasons.dedup();
    for season in seasons {
        let (have,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM episode WHERE title_id = ? AND season_number = ?")
                .bind(title_id)
                .bind(season)
                .fetch_one(&state.pool)
                .await?;
        if have == 0 {
            catalog::sync_all_seasons(state, title_id, detail).await?;
            break;
        }
    }

    // One commit for the lot: a long run of single writes is slow.
    let mut tx = state.pool.begin().await?;
    for e in episodes {
        let (at, undated) = match e.watched_at.as_deref() {
            Some(at) => (at.to_string(), 0),
            None => (chrono::Utc::now().to_rfc3339(), 1),
        };
        if undated == 0 {
            sqlx::query(
                "UPDATE watch_event SET watched_at = ?1, undated = 0
                  WHERE user_id = ?2 AND source = 'import' AND undated = 1
                    AND episode_id = (SELECT id FROM episode
                                       WHERE title_id = ?3 AND season_number = ?4
                                         AND episode_number = ?5)",
            )
            .bind(&at)
            .bind(user_id)
            .bind(title_id)
            .bind(e.season)
            .bind(e.episode)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch,
                                      source, undated)
             SELECT ?1, ?2, e.id, ?3, 0, 'import', ?4
               FROM episode e
              WHERE e.title_id = ?2 AND e.season_number = ?5 AND e.episode_number = ?6
                AND NOT EXISTS (
                      SELECT 1 FROM watch_event w
                       WHERE w.user_id = ?1 AND w.episode_id = e.id)",
        )
        .bind(user_id)
        .bind(title_id)
        .bind(at)
        .bind(undated)
        .bind(e.season)
        .bind(e.episode)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(extra: Value) -> ApplyItem {
        let mut v = json!({ "kind": "tv", "tmdb_id": 1 });
        for (k, val) in extra.as_object().unwrap() {
            v[k] = val.clone();
        }
        serde_json::from_value(v).unwrap()
    }

    fn entry(status: &str, rating: Option<f64>, favourite: bool) -> LibraryEntry {
        LibraryEntry {
            status: status.into(),
            rating,
            favourite,
            notes: None,
            started_at: None,
            finished_at: None,
            rewatch_count: 0,
            added_at: String::new(),
            updated_at: String::new(),
            notify: true,
        }
    }

    #[test]
    fn a_merge_only_moves_a_status_forward() {
        assert_eq!(merged_status("planned", "watching"), "watching");
        assert_eq!(merged_status("watching", "completed"), "completed");
        assert_eq!(merged_status("completed", "watching"), "completed");
        assert_eq!(merged_status("watching", "planned"), "watching");
        // Your own pause or drop is never undone by a tracker.
        assert_eq!(merged_status("paused", "completed"), "paused");
        assert_eq!(merged_status("dropped", "watching"), "dropped");
        assert_eq!(merged_status("planned", "dropped"), "planned");
    }

    #[test]
    fn an_unknown_status_arrives_as_completed() {
        assert_eq!(incoming_status(Some("paused")), "paused");
        assert_eq!(incoming_status(Some("finished")), "completed");
        assert_eq!(incoming_status(None), "completed");
    }

    #[test]
    fn a_merge_keeps_yours_and_fills_the_gaps_while_overwrite_takes_the_file() {
        let row = item(json!({ "rating": 50.0, "favourite": false }));
        let mine = entry("watching", Some(90.0), true);
        let p = patch_for(Some(&mine), "completed".into(), &row);
        assert_eq!(
            (p.status.as_deref(), p.rating, p.favourite),
            (Some("completed"), Some(Some(90.0)), Some(true))
        );
        let empty = entry("paused", None, false);
        let p = patch_for(Some(&empty), "completed".into(), &row);
        assert_eq!(
            (p.status.as_deref(), p.rating),
            (Some("paused"), Some(Some(50.0)))
        );
        let p = patch_for(None, "planned".into(), &item(json!({})));
        assert_eq!(
            (p.status.as_deref(), p.rating, p.favourite),
            (Some("planned"), Some(None), Some(false))
        );
    }

    #[test]
    fn a_season_row_ticks_from_its_offset() {
        let span = |extra: Value, completed| season_span(&item(extra), completed);
        assert_eq!(span(json!({ "progress": 7 }), false), Some((0, 7)));
        assert_eq!(
            span(json!({ "episode_offset": 12, "progress": 4 }), false),
            Some((12, 16))
        );
        assert_eq!(
            span(json!({ "episode_offset": 12, "episode_total": 12 }), true),
            Some((12, 24))
        );
        assert_eq!(span(json!({}), true), Some((0, i64::MAX)));
        assert_eq!(span(json!({}), false), None);
        assert_eq!(span(json!({ "progress": 0 }), true), None);
        assert_eq!(
            span(json!({ "episode_offset": -3, "progress": 2 }), false),
            Some((0, 2))
        );
    }
}
