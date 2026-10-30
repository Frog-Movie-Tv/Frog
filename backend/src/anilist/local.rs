//! Entries TMDB does not have (yet), kept in the library all the same as
//! AniList-only titles: catalog titles whose provider is AniList, drawn from
//! AniList by `media`. When TMDB places the entry later, `adopt_local`
//! moves this account's data across.

use chrono::Utc;
use sqlx::SqlitePool;

use super::client::{push, Remote, View};
use super::library::{self, film_watch, frog_view, run_ids, settle_show};
use super::place::Target;
use super::reconcile::{self, Changes, Mode};
use super::records::{save_snapshots, Known};
use super::Account;
use crate::catalog::{self, ExternalId};
use crate::error::AppResult;
use crate::release;
use crate::state::AppState;
use crate::store::{self, LibraryPatch};

/// The words TMDB uses for a show's state, for AniList's.
pub(super) fn local_status(media_status: Option<&str>) -> Option<&'static str> {
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
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT title_id FROM title_external
          WHERE provider = 'anilist' AND external_id = ? AND is_primary = 1",
    )
    .bind(ExternalId::AniList(media_id).key())
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}

/// The episodes kept as a count for an AniList-only title of this account,
/// when it has no episodes to tick.
async fn counted_progress(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
) -> AppResult<Option<i64>> {
    let row: Option<(Option<i64>,)> =
        sqlx::query_as("SELECT progress FROM library_entry WHERE user_id = ? AND title_id = ?")
            .bind(user_id)
            .bind(title_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|r| r.0))
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
    if film {
        let watches = library::film_watches(pool, user_id, title_id).await?;
        return Ok(Some(reconcile::film_view(
            &entry.status,
            entry.rating,
            watches,
        )));
    }
    Ok(Some(View {
        status: reconcile::frog_to_anilist(&entry.status).into(),
        progress: counted_progress(pool, user_id, title_id)
            .await?
            .unwrap_or(0),
        score: reconcile::score_of(entry.rating),
    }))
}

/// Keep an entry TMDB cannot place (yet) in the library all the same, as a
/// title of its own drawn from AniList (`media`): poster, synopsis, cast,
/// episodes with their air dates. Where AniList lists the episodes, they are
/// ticked and synced like any show's; where it does not (a film, or a show
/// with nothing known yet), the count of episodes is kept instead. Status and
/// score go both ways. Says whether Frog changed and whether AniList was
/// written to.
pub(super) async fn sync_local(
    acct: &Account<'_>,
    remote: &Remote,
    prior: Option<&Known>,
) -> AppResult<(bool, bool)> {
    let (state, user_id) = (acct.state, acct.user_id);
    let film = remote.format.as_deref() == Some("MOVIE");
    let kind = if film { "movie" } else { "tv" };
    let original = remote
        .titles
        .iter()
        .find(|t| **t != remote.name)
        .map(String::as_str);
    let title_id = catalog::ensure_local_title(
        &state.pool,
        &catalog::LocalTitle {
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
        if let Err(e) = catalog::ensure(state, &ExternalId::AniList(remote.media_id)).await {
            tracing::debug!(media_id = remote.media_id, error = %e, "AniList detail not read yet");
        }
    }
    let (episodes,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM episode WHERE title_id = ? AND season_number > 0")
            .bind(title_id)
            .fetch_one(&state.pool)
            .await?;
    if !film && episodes > 0 {
        return sync_local_episodes(acct, remote, prior, title_id).await;
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
    let c = Changes::between(snap_al.as_ref(), snap_frog.as_ref(), a, f.as_ref());

    if c.first || c.al_changed {
        let mode = if c.first { Mode::Merge } else { Mode::Follow };
        let existing = store::get_entry(&state.pool, user_id, title_id).await?;
        let status = reconcile::incoming_status(
            existing.as_ref().map(|e| e.status.as_str()),
            reconcile::anilist_to_frog(&a.status),
            mode,
        );
        let rating = reconcile::incoming_rating(
            existing.as_ref().and_then(|e| e.rating),
            a.score,
            mode,
            snap_al.as_ref(),
        );
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
            let progress = if c.first {
                have.max(a.progress)
            } else {
                a.progress
            };
            store::set_local_progress(&state.pool, user_id, title_id, progress).await?;
        }
    }
    let f_now = local_view(&state.pool, user_id, title_id, film).await?;

    // Changed only in Frog: its status and score go to AniList. The count
    // stays AniList's, since there are no episodes here to count from.
    let mut final_al = a.clone();
    let mut pushed = false;
    if let (Some(now), true) = (f_now.as_ref(), c.frog_changed && !c.al_changed) {
        let send = View {
            status: now.status.clone(),
            progress: a.progress,
            score: now.score,
        };
        if &send != a {
            final_al = push(acct.api, acct.token, remote.media_id, &send).await?;
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
async fn sync_local_episodes(
    acct: &Account<'_>,
    remote: &Remote,
    prior: Option<&Known>,
    title_id: i64,
) -> AppResult<(bool, bool)> {
    let (state, user_id) = (acct.state, acct.user_id);
    // The whole of AniList's own numbering, in the title Frog keeps for it.
    // There is no TMDB entry, and nothing here reads one.
    let target = Target::Show {
        tmdb_id: 0,
        season: 1,
        offset: 0,
    };
    // Kept as a count before its episodes were known: laid out afresh, once.
    let counted = counted_progress(&state.pool, user_id, title_id).await?;
    let (snap_al, snap_frog) = match prior {
        Some(k) if k.state == "unsure" && counted.is_none() => (k.al.clone(), k.frog.clone()),
        _ => (None, None),
    };
    let a = &remote.view;
    let view = |pool| {
        frog_view(
            pool,
            user_id,
            remote.media_id,
            title_id,
            target,
            remote.episodes,
        )
    };
    let f = view(&state.pool).await?;
    if f.is_none() && snap_al.is_some() {
        save_snapshots(&state.pool, user_id, remote.media_id, a, None, true).await?;
        return Ok((false, false));
    }
    let c = Changes::between(snap_al.as_ref(), snap_frog.as_ref(), a, f.as_ref());
    match c.mode(false) {
        Some(mode) => {
            let created = library::apply(
                state,
                user_id,
                remote,
                title_id,
                target,
                mode,
                snap_al.as_ref(),
            )
            .await?;
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
    let f_now = view(&state.pool).await?;

    let mut final_al = a.clone();
    let mut pushed = false;
    if let Some(now) = f_now.as_ref() {
        if let Some(send) = reconcile::outgoing(c.first, &c, snap_frog.as_ref(), now, a) {
            final_al = push(acct.api, acct.token, remote.media_id, &send).await?;
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
pub(super) async fn adopt_local(
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
        let (_, detail) = catalog::ensure(state, &ExternalId::tmdb("tv", tmdb_id)).await?;
        catalog::ensure_episodes(state, title_id, Some(&detail)).await?;
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
pub(super) fn next_try(tries: i64, upcoming: bool) -> String {
    let cap = if upcoming { 12 } else { 72 };
    let hours = (1i64 << tries.clamp(0, 7)).min(cap);
    (Utc::now() + chrono::Duration::hours(hours)).to_rfc3339()
}
