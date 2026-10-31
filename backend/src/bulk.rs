//! One action on many titles of one account's library, and the marking a show
//! shares with it: Library's edit mode and "Mark all watched" tick the same
//! episodes by the same rules.

use chrono::Utc;
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::catalog;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::store::{self, EntryRecord, LibraryPatch};

/// How many titles one request may name. A library is about this size at the
/// very most; the list page reads no more than this either.
pub const MAX_TITLES: usize = 10_000;

/// When a batch of episodes was watched.
///
/// Marking a whole back catalogue as watched is a statement about the past,
/// and stamping every episode with today's date flooded the diary with
/// hundreds of entries that never happened. So bulk marking asks.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum When {
    /// Just now.
    #[default]
    Today,
    /// Each episode on the day it aired; undated if the air date is unknown.
    Release,
    /// One chosen day for all of them.
    Date,
    /// Watched, but no idea when: counted everywhere, kept out of the diary.
    Unknown,
}

/// The answer to "When did you watch this?".
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Marking {
    #[serde(default)]
    pub when: When,
    /// yyyy-mm-dd, required when `when` is `date`.
    pub date: Option<String>,
}

impl Marking {
    fn mode(&self) -> &'static str {
        match self.when {
            When::Today => "today",
            When::Release => "release",
            When::Date => "date",
            When::Unknown => "unknown",
        }
    }

    /// The chosen day as a watch's timestamp, checked. Asked before anything
    /// is written, so a bad date changes nothing.
    fn chosen(&self) -> AppResult<Option<String>> {
        match self.when {
            When::Date => Ok(Some(evening_of(self.date.as_deref().unwrap_or(""))?)),
            _ => Ok(None),
        }
    }
}

/// Turn a yyyy-mm-dd into the timestamp watches are stored with, refusing
/// anything that is not a real calendar date or lies in the future. Today is
/// the person's, and tomorrow by UTC can already be that somewhere.
pub fn evening_of(date: &str) -> AppResult<String> {
    let day = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("pick a valid date".into()))?;
    if day > Utc::now().date_naive() + chrono::Days::new(1) {
        return Err(AppError::BadRequest("that date is in the future".into()));
    }
    Ok(format!("{date}T20:00:00Z"))
}

/// Whether a film can have been watched. One with no date is given the
/// benefit of the doubt; one dated in the future has not come out.
pub fn film_is_out(release_date: Option<&str>, today: &str) -> bool {
    release_date.is_none_or(|d| d <= today)
}

/// When a film's first watch happened, and whether that is undated.
pub fn film_watch(
    when: When,
    release_date: Option<&str>,
    chosen: Option<&str>,
    now: &str,
) -> (String, bool) {
    match (when, release_date, chosen) {
        (When::Release, Some(day), _) => (format!("{day}T20:00:00Z"), false),
        (When::Release, None, _) | (When::Unknown, _, _) => (now.to_string(), true),
        (When::Date, _, Some(chosen)) => (chosen.to_string(), false),
        (When::Date | When::Today, _, _) => (now.to_string(), false),
    }
}

/// Add a watch for each aired episode with none yet, of one season or, with
/// `season` as `None`, of the whole show bar its specials. Only episodes with
/// no watch, so this never inflates rewatches. `specials` are ticked too, by
/// number, when marking a whole show: a finished show means its seasons, and
/// specials are chosen.
pub async fn tick_aired(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    season: Option<i64>,
    marking: &Marking,
    specials: &[i64],
) -> AppResult<()> {
    let mode = marking.mode();
    let chosen = marking.chosen()?;

    sqlx::query(concat!(
        "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch,
                                  source, undated)
         SELECT ?6, ?1, e.id,
                CASE ?2 WHEN 'release' THEN COALESCE(e.air_date || 'T20:00:00Z', ?3)
                        WHEN 'date' THEN ?4
                        ELSE ?3 END,
                0, 'manual',
                CASE ?2 WHEN 'unknown' THEN 1
                        WHEN 'release' THEN e.air_date IS NULL
                        ELSE 0 END
           FROM episode e
          WHERE e.title_id = ?1
            AND (?5 IS NOT NULL OR e.season_number > 0)
            AND (?5 IS NULL OR e.season_number = ?5)
            AND ",
        crate::released!(),
        "
            AND NOT EXISTS (
                  SELECT 1 FROM watch_event w
                   WHERE w.user_id = ?6 AND w.episode_id = e.id)"
    ))
    .bind(title_id)
    .bind(mode)
    .bind(Utc::now().to_rfc3339())
    .bind(chosen.as_deref())
    .bind(season)
    .bind(user_id)
    .execute(pool)
    .await?;

    if season.is_none() {
        for special in specials {
            sqlx::query(concat!(
                "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch,
                                          source, undated)
                 SELECT ?6, ?1, e.id,
                        CASE ?2 WHEN 'release' THEN COALESCE(e.air_date || 'T20:00:00Z', ?3)
                                WHEN 'date' THEN ?4
                                ELSE ?3 END,
                        0, 'manual',
                        CASE ?2 WHEN 'unknown' THEN 1
                                WHEN 'release' THEN e.air_date IS NULL
                                ELSE 0 END
                   FROM episode e
                  WHERE e.title_id = ?1 AND e.season_number = 0 AND e.episode_number = ?5
                    AND ",
                crate::released!(),
                "
                    AND NOT EXISTS (
                          SELECT 1 FROM watch_event w
                           WHERE w.user_id = ?6 AND w.episode_id = e.id)"
            ))
            .bind(title_id)
            .bind(mode)
            .bind(Utc::now().to_rfc3339())
            .bind(chosen.as_deref())
            .bind(special)
            .bind(user_id)
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}

/// What marking titles watched did.
#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct Marked {
    pub marked: i64,
    /// Nothing out yet to have watched, or the episodes could not be listed.
    pub skipped: i64,
}

/// Mark titles of a library watched: a show's aired episodes, by the same
/// rules as "Mark all watched"; a film as finished, with its first watch.
/// `finish` also says the status is Completed whatever it was, which is what
/// choosing that status means; otherwise a paused or dropped show keeps it.
pub async fn mark_watched(
    state: &AppState,
    user_id: i64,
    title_ids: &[i64],
    marking: &Marking,
    finish: bool,
) -> AppResult<Marked> {
    let pool = &state.pool;
    let chosen = marking.chosen()?;
    let now = Utc::now();
    let (stamp, today) = (now.to_rfc3339(), now.format("%Y-%m-%d").to_string());
    let completed = || LibraryPatch {
        status: Some("completed".into()),
        ..Default::default()
    };

    let mut out = Marked::default();
    for title in store::tracked_titles(pool, user_id, title_ids).await? {
        if title.kind == "movie" {
            if !film_is_out(title.release_date.as_deref(), &today) {
                out.skipped += 1;
                continue;
            }
            store::apply_patch(pool, user_id, title.id, &completed()).await?;
            let (at, undated) = film_watch(
                marking.when,
                title.release_date.as_deref(),
                chosen.as_deref(),
                &stamp,
            );
            store::add_first_film_watch(pool, user_id, title.id, &at, undated).await?;
            out.marked += 1;
            continue;
        }

        if let Err(e) = catalog::ensure_episodes(state, title.id, None).await {
            tracing::warn!(title = title.id, error = %e, "could not list its episodes");
            out.skipped += 1;
            continue;
        }
        if finish {
            store::apply_patch(pool, user_id, title.id, &completed()).await?;
        }
        tick_aired(pool, user_id, title.id, None, marking, &[]).await?;
        crate::release::reconcile(pool, user_id, title.id).await?;
        if store::progress(pool, user_id, title.id).await?.total > 0 {
            out.marked += 1;
        } else {
            out.skipped += 1;
        }
    }
    Ok(out)
}

/// Take titles out of a library, and say what they held, so undo can put back
/// every field as it was. Watches, shelves and season ratings stay, as they do
/// when one title is removed.
pub async fn remove(
    pool: &SqlitePool,
    user_id: i64,
    title_ids: &[i64],
) -> AppResult<Vec<EntryRecord>> {
    let held = store::entry_records(pool, user_id, title_ids).await?;
    let ids: Vec<i64> = held.iter().map(|e| e.title_id).collect();
    store::remove_entries(pool, user_id, &ids).await?;
    Ok(held)
}

/// Move titles to a status other than Completed, which goes by `mark_watched`.
/// Says what they held before, for undo.
pub async fn set_status(
    pool: &SqlitePool,
    user_id: i64,
    title_ids: &[i64],
    status: &str,
) -> AppResult<Vec<EntryRecord>> {
    edit(pool, user_id, title_ids, |_| LibraryPatch {
        status: Some(status.to_string()),
        ..Default::default()
    })
    .await
}

/// Say what titles held before, for undo.
pub async fn set_favourite(
    pool: &SqlitePool,
    user_id: i64,
    title_ids: &[i64],
    favourite: bool,
) -> AppResult<Vec<EntryRecord>> {
    edit(pool, user_id, title_ids, |_| LibraryPatch {
        favourite: Some(favourite),
        ..Default::default()
    })
    .await
}

async fn edit(
    pool: &SqlitePool,
    user_id: i64,
    title_ids: &[i64],
    patch: impl Fn(&EntryRecord) -> LibraryPatch,
) -> AppResult<Vec<EntryRecord>> {
    let held = store::entry_records(pool, user_id, title_ids).await?;
    for entry in &held {
        store::apply_patch(pool, user_id, entry.title_id, &patch(entry)).await?;
    }
    Ok(held)
}

/// Put titles on a hand-picked shelf of this account's. Only titles in the
/// library, and each only once. Says how many are new there.
pub async fn add_to_shelf(
    pool: &SqlitePool,
    user_id: i64,
    shelf_id: i64,
    title_ids: &[i64],
) -> AppResult<u64> {
    let kind: Option<(String,)> =
        sqlx::query_as("SELECT kind FROM shelf WHERE id = ? AND user_id = ?")
            .bind(shelf_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    match kind {
        None => return Err(AppError::NotFound("no such shelf".into())),
        Some((kind,)) if kind != "manual" => {
            return Err(AppError::BadRequest(
                "A smart shelf fills itself from its rules.".into(),
            ))
        }
        Some(_) => {}
    }
    let mut added = 0;
    let mut tx = pool.begin().await?;
    for title in store::tracked_titles(pool, user_id, title_ids).await? {
        added += sqlx::query(
            "INSERT OR IGNORE INTO shelf_item (shelf_id, title_id, position)
             VALUES (?, ?, (SELECT COALESCE(MAX(position), -1) + 1 FROM shelf_item
                             WHERE shelf_id = ?))",
        )
        .bind(shelf_id)
        .bind(title.id)
        .bind(shelf_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    }
    tx.commit().await?;
    Ok(added)
}

/// Refuse a request naming nothing, or more than one library holds.
pub fn check_size(title_ids: &[i64]) -> AppResult<()> {
    if title_ids.is_empty() {
        return Err(AppError::BadRequest("choose at least one title".into()));
    }
    if title_ids.len() > MAX_TITLES {
        return Err(AppError::BadRequest("too many titles at once".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-10-31T12:00:00+00:00";

    #[test]
    fn a_film_not_out_yet_cannot_have_been_watched() {
        assert!(film_is_out(Some("2026-10-31"), "2026-10-31"));
        assert!(!film_is_out(Some("2026-11-01"), "2026-10-31"));
        assert!(
            film_is_out(None, "2026-10-31"),
            "no date, no way of knowing"
        );
    }

    #[test]
    fn a_film_watch_lands_where_the_answer_says() {
        let day = Some("1995-12-15");
        assert_eq!(
            film_watch(When::Release, day, None, NOW),
            ("1995-12-15T20:00:00Z".to_string(), false)
        );
        assert_eq!(
            film_watch(When::Release, None, None, NOW),
            (NOW.to_string(), true),
            "no release date to put it on, so it stays out of the diary"
        );
        assert_eq!(
            film_watch(When::Date, day, Some("2001-02-03T20:00:00Z"), NOW),
            ("2001-02-03T20:00:00Z".to_string(), false)
        );
        assert_eq!(
            film_watch(When::Today, day, None, NOW),
            (NOW.to_string(), false)
        );
        assert_eq!(
            film_watch(When::Unknown, day, None, NOW),
            (NOW.to_string(), true)
        );
    }

    #[test]
    fn a_date_has_to_be_real_and_not_ahead() {
        assert!(evening_of("2001-02-03").is_ok());
        assert!(evening_of("2001-02-31").is_err());
        assert!(evening_of("3000-01-01").is_err());
        assert!(Marking {
            when: When::Date,
            date: None
        }
        .chosen()
        .is_err());
    }

    #[test]
    fn a_request_names_something_and_not_too_much() {
        assert!(check_size(&[]).is_err());
        assert!(check_size(&[1]).is_ok());
        assert!(check_size(&vec![1; MAX_TITLES + 1]).is_err());
    }
}
