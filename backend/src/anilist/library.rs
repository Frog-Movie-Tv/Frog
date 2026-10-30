//! Frog's side of an AniList entry: which episodes it covers, how Frog sees
//! it, and writing AniList's side into the library. The decisions come from
//! `reconcile`; this module only reads and writes.

use std::collections::HashSet;

use chrono::Utc;
use sqlx::SqlitePool;

use super::client::{Remote, View};
use super::place::Target;
use super::reconcile::{self, Mode};
use super::records;
use crate::catalog::{self, ExternalId};
use crate::error::AppResult;
use crate::release;
use crate::state::AppState;
use crate::store::{self, LibraryPatch};

/// The episodes an entry covers, in order, with whether each has come out:
/// from `offset` into `season`, carrying on into later seasons. An entry
/// whose length AniList does not know (a long series still airing) runs on
/// until the next AniList entry of the same show, or to the end of the show,
/// never just to the end of a TMDB season. Specials stay within season 0.
pub(super) async fn run_ids(
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
    let others = records::others_in(pool, user_id, title_id, media_id, true).await?;
    let order: Vec<(i64, i64)> = rows.iter().map(|r| (r.1, r.2)).collect();
    let mut spans = vec![(season, offset, episodes)];
    spans.extend(others.into_iter().map(|(_, s, o, n)| (s, o, n)));
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

/// How many times a film has been watched, as title-level watches.
pub(super) async fn film_watches(pool: &SqlitePool, user_id: i64, title_id: i64) -> AppResult<i64> {
    let (watches,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM watch_event
          WHERE user_id = ? AND title_id = ? AND episode_id IS NULL",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_one(pool)
    .await?;
    Ok(watches)
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
pub(super) async fn frog_view(
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
            let watches = film_watches(pool, user_id, title_id).await?;
            reconcile::film_view(&entry.status, entry.rating, watches)
        }
        Target::Show { season, offset, .. } => {
            let run = run_ids(pool, user_id, media_id, title_id, season, offset, episodes).await?;
            let ids: Vec<i64> = run.iter().map(|r| r.0).collect();
            let progress = watched_among(pool, user_id, &ids).await?;
            // Each AniList entry is scored on its own, like each season here.
            let rating = part_rating(pool, user_id, title_id, season, offset).await?;
            reconcile::show_view(&entry.status, progress, episodes, rating)
        }
    };
    Ok(Some(view))
}

/// Before a person moves an entry somewhere else, take back what its old
/// place was given from AniList; the next sync lays it out afresh.
pub async fn forget_place(state: &AppState, user_id: i64, media_id: i64) -> AppResult<()> {
    let row = records::placement(&state.pool, user_id, media_id).await?;
    let Some((Some(title_id), season, offset, episodes, Some(key))) = row else {
        return Ok(());
    };
    let Some(ExternalId::Tmdb { kind, id: tmdb_id }) = ExternalId::parse("tmdb", &key) else {
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

/// Make Frog match AniList for one entry, placed at `target` in the title
/// `title_id` (which the caller has already put in the catalog). Says
/// whether the title was new to the library, in which case its status is
/// AniList's outright.
pub(super) async fn apply(
    state: &AppState,
    user_id: i64,
    remote: &Remote,
    title_id: i64,
    target: Target,
    mode: Mode,
    before: Option<&View>,
) -> AppResult<bool> {
    let kind = match target {
        Target::Movie(_) => "movie",
        Target::Show { .. } => "tv",
    };
    let existing = store::get_entry(&state.pool, user_id, title_id).await?;
    let a = &remote.view;

    let current = match target {
        Target::Movie(_) => existing.as_ref().and_then(|e| e.rating),
        Target::Show { season, offset, .. } => {
            part_rating(&state.pool, user_id, title_id, season, offset).await?
        }
    };
    let rating = reconcile::incoming_rating(current, a.score, mode, before);

    let incoming = reconcile::anilist_to_frog(&a.status);
    let status = match (&existing, kind) {
        // A show's status follows its seasons together; see `settle_show`.
        (None, "tv") => Some("planned".to_string()),
        (Some(_), "tv") => None,
        (e, _) => Some(reconcile::incoming_status(
            e.as_ref().map(|e| e.status.as_str()),
            incoming,
            mode,
        )),
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
            catalog::ensure_episodes(state, title_id, None).await?;
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
pub(super) async fn film_watch(pool: &SqlitePool, user_id: i64, title_id: i64) -> AppResult<()> {
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

/// Take back what an earlier, wrong placement of an entry ticked, so moving
/// it does not leave its progress behind in the wrong show or season. Only
/// ticks that came from AniList; anything ticked by hand stays.
pub(super) async fn unplace(
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
            let others =
                records::others_in(&state.pool, user_id, title_id, media_id, false).await?;
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
pub(super) async fn settle_show(
    state: &AppState,
    user_id: i64,
    title_id: i64,
    entries: &[(i64, i64, String)],
    mode: Mode,
) -> AppResult<()> {
    let Some(entry) = store::get_entry(&state.pool, user_id, title_id).await? else {
        return Ok(());
    };
    let status = reconcile::show_status(&entry.status, entries, mode);
    if status != entry.status {
        let patch = LibraryPatch {
            status: Some(status),
            ..Default::default()
        };
        store::apply_patch(&state.pool, user_id, title_id, &patch).await?;
    }
    release::reconcile(&state.pool, user_id, title_id).await
}
