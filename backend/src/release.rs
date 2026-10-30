//! What has come out, and what that means for where someone is with a show.
//!
//! One set of rules for every path that moves a status: ticking an episode,
//! an AniList sync, and the worker noticing new episodes. Nothing that has
//! not been released is ever watchable, and a finished season stays finished
//! when a sequel is announced.

use serde::Serialize;
use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::store;

/// SQL condition: episode `e` has come out. TMDB lists announced episodes
/// with no date, so an undated one only counts when a later one has aired;
/// treating every undated episode as out is what made a season 2 that does
/// not exist yet show up as the next thing to watch.
/// The same condition, for splicing into a query with `concat!`.
#[macro_export]
macro_rules! released {
    () => {
        "(e.air_date <= date('now') OR (e.air_date IS NULL AND EXISTS (
        SELECT 1 FROM episode later
         WHERE later.title_id = e.title_id AND later.season_number > 0
           AND (later.season_number > e.season_number
                OR (later.season_number = e.season_number
                    AND later.episode_number > e.episode_number))
           AND later.air_date <= date('now'))))"
    };
}
pub const RELEASED: &str = released!();

/// One regular (non-special) episode of a show, in viewing order.
#[derive(Debug, Clone)]
pub struct Episode {
    pub season: i64,
    pub episode: i64,
    pub air_date: Option<String>,
    pub released: bool,
    pub watched: bool,
}

/// Where somebody stands with a show.
#[derive(Debug, Clone, Default)]
pub struct ShowState {
    /// Regular episodes out so far.
    pub released: i64,
    /// Of those, the ones watched.
    pub watched: i64,
    /// More of the run that is under way is coming: the next episode of the
    /// same season (or AniList entry), not a sequel.
    pub ongoing: bool,
}

/// (season, episode, air date, released, watched)
type EpisodeRow = (i64, i64, Option<String>, bool, bool);

/// Every regular episode of a show, in order, with what has come out and what
/// this person has watched.
pub async fn episodes(pool: &SqlitePool, user_id: i64, title_id: i64) -> AppResult<Vec<Episode>> {
    let sql = format!(
        "SELECT e.season_number, e.episode_number, e.air_date, {RELEASED},
                EXISTS (SELECT 1 FROM watch_event w WHERE w.user_id = ?1 AND w.episode_id = e.id)
           FROM episode e
          WHERE e.title_id = ?2 AND e.season_number > 0
          ORDER BY e.season_number, e.episode_number"
    );
    let rows: Vec<EpisodeRow> = sqlx::query_as(&sql)
        .bind(user_id)
        .bind(title_id)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|(season, episode, air_date, released, watched)| Episode {
            season,
            episode,
            air_date,
            released,
            watched,
        })
        .collect())
}

/// Where each AniList entry starts and ends in a show's episode order.
/// `entries` are (season, offset, episode count); an entry whose length
/// AniList does not know runs until the next entry starts, or to the end of
/// the show, rather than stopping at the end of a TMDB season: a long series
/// with one AniList entry and twenty TMDB seasons is still one run.
pub fn ranges(order: &[(i64, i64)], entries: &[(i64, i64, Option<i64>)]) -> Vec<(usize, usize)> {
    let starts: Vec<usize> = entries
        .iter()
        .map(|&(season, offset, _)| {
            order
                .iter()
                .position(|&(s, e)| s > season || (s == season && e > offset))
                .unwrap_or(order.len())
        })
        .collect();
    entries
        .iter()
        .zip(&starts)
        .map(|(&(_, _, length), &start)| {
            let end = match length {
                Some(n) if n > 0 => start + n as usize,
                _ => starts
                    .iter()
                    .copied()
                    .filter(|&s| s > start)
                    .min()
                    .unwrap_or(order.len()),
            };
            (start, end.min(order.len()))
        })
        .collect()
}

/// (season, offset, episodes, AniList media status)
type CoverRow = (i64, i64, Option<i64>, Option<String>);

/// Where somebody stands with a show, and whether what comes next continues
/// the run they are in or starts a new one.
pub async fn show_state(pool: &SqlitePool, user_id: i64, title_id: i64) -> AppResult<ShowState> {
    let list = episodes(pool, user_id, title_id).await?;
    let released = list.iter().filter(|e| e.released).count() as i64;
    let watched = list.iter().filter(|e| e.released && e.watched).count() as i64;
    let Some(last) = list.iter().rposition(|e| e.released) else {
        return Ok(ShowState {
            released,
            watched,
            ongoing: false,
        });
    };

    // AniList knows best whether the entry you are in is still airing.
    let covers: Vec<CoverRow> = sqlx::query_as(
        "SELECT season, episode_offset, episodes, media_status FROM anilist_entry
          WHERE user_id = ? AND title_id = ? AND state = 'placed' AND season > 0",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_all(pool)
    .await?;
    let order: Vec<(i64, i64)> = list.iter().map(|e| (e.season, e.episode)).collect();
    let spans: Vec<(i64, i64, Option<i64>)> = covers.iter().map(|c| (c.0, c.1, c.2)).collect();
    let covering = ranges(&order, &spans)
        .into_iter()
        .zip(&covers)
        .find(|((start, end), _)| *start <= last && last < *end)
        .and_then(|(_, c)| c.3.clone());

    let ongoing = match covering.as_deref() {
        Some("RELEASING" | "HIATUS") => true,
        Some(_) => false,
        None => tmdb_ongoing(pool, title_id, &list, last).await?,
    };
    Ok(ShowState {
        released,
        watched,
        ongoing,
    })
}

/// Without AniList: TMDB's next episode continuing the same season means
/// the season is still airing; an episode 1, or nothing, means it is over.
async fn tmdb_ongoing(
    pool: &SqlitePool,
    title_id: i64,
    list: &[Episode],
    last: usize,
) -> AppResult<bool> {
    let current = list[last].season;
    let (season, episode): (Option<i64>, Option<i64>) =
        sqlx::query_as("SELECT next_air_season, next_air_episode FROM title WHERE id = ?")
            .bind(title_id)
            .fetch_one(pool)
            .await?;
    if let (Some(season), Some(episode)) = (season, episode) {
        return Ok(season == current && episode > 1);
    }
    // No word from TMDB: a dated episode of the same season due within three
    // weeks is the same run carrying on.
    let soon = (chrono::Utc::now() + chrono::Duration::days(21))
        .format("%Y-%m-%d")
        .to_string();
    Ok(list[last + 1..]
        .iter()
        .any(|e| e.season == current && e.air_date.as_deref().is_some_and(|d| d <= soon.as_str())))
}

/// The status a show should have, or `None` to leave it as it is.
///
/// - Nothing released: never Watching; it is something to plan.
/// - Released episodes left: Watching (a new season of a finished show too).
/// - All of it watched and the same run continues: Watching, which Home
///   shows as caught up.
/// - All of it watched and nothing else of this run is coming: Completed,
///   when the person lets Frog complete shows, whatever sequel is announced.
/// - Paused and dropped are the person's word and are never moved.
pub fn desired_status(
    current: Option<&str>,
    state: &ShowState,
    auto_complete: bool,
) -> Option<&'static str> {
    if matches!(current, Some("paused" | "dropped")) {
        return None;
    }
    let want = if state.released == 0 {
        // Not out: a show marked watching with nothing seen is only planned.
        (current == Some("watching") && state.watched == 0).then_some("planned")?
    } else if state.watched == 0 {
        return None;
    } else if state.watched < state.released || state.ongoing {
        // Episodes left, or caught up on a run that carries on.
        "watching"
    } else if auto_complete {
        "completed"
    } else {
        return None;
    };
    (current != Some(want)).then_some(want)
}

/// Something that can be rated on its own: a season, or one AniList entry
/// where AniList splits a TMDB season into several (or runs one entry across
/// many TMDB seasons, as a long series does).
#[derive(Debug, Clone, Serialize)]
pub struct RatingUnit {
    pub season: i64,
    pub first_episode: i64,
    /// Episodes in it that TMDB lists.
    pub episodes: i64,
    /// AniList's name for it, when it is an AniList entry.
    pub name: Option<String>,
    pub rating: Option<f64>,
    /// All of it is out, so it can be judged as a whole.
    pub rateable: bool,
}

/// (season, offset, episodes, name, AniList media status)
type PartRow = (i64, i64, Option<i64>, String, Option<String>);

/// The seasons and parts of a show that can each carry a rating, with the
/// rating given and whether one can be given yet. Ratings already given are
/// always listed, so nothing is hidden by a change in how a show is split.
pub async fn rating_units(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
) -> AppResult<Vec<RatingUnit>> {
    let list = episodes(pool, user_id, title_id).await?;
    let order: Vec<(i64, i64)> = list.iter().map(|e| (e.season, e.episode)).collect();
    let parts: Vec<PartRow> = sqlx::query_as(
        "SELECT season, episode_offset, episodes, name, media_status FROM anilist_entry
          WHERE user_id = ? AND title_id = ? AND state = 'placed' AND season > 0
          ORDER BY season, episode_offset",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_all(pool)
    .await?;
    let rated: Vec<(i64, i64, f64)> = sqlx::query_as(
        "SELECT season_number, first_episode, rating FROM season_rating
          WHERE user_id = ? AND title_id = ?",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_all(pool)
    .await?;
    let (next_season,): (Option<i64>,) =
        sqlx::query_as("SELECT next_air_season FROM title WHERE id = ?")
            .bind(title_id)
            .fetch_one(pool)
            .await?;
    let rating_of = |season: i64, first: i64| {
        rated
            .iter()
            .find(|r| r.0 == season && r.1 == first)
            .map(|r| r.2)
    };

    let spans: Vec<(i64, i64, Option<i64>)> = parts.iter().map(|p| (p.0, p.1, p.2)).collect();
    let mut covered = vec![false; list.len()];
    let mut units: Vec<RatingUnit> = Vec::new();
    for ((start, end), part) in ranges(&order, &spans).into_iter().zip(&parts) {
        if start >= end {
            continue;
        }
        covered[start..end].iter_mut().for_each(|c| *c = true);
        let all_out = list[start..end].iter().all(|e| e.released);
        let finished = match part.4.as_deref() {
            Some("FINISHED") => true,
            Some(_) => false,
            None => part.2.is_some_and(|n| n as usize == end - start),
        };
        units.push(RatingUnit {
            season: part.0,
            first_episode: part.1 + 1,
            episodes: (end - start) as i64,
            name: Some(part.3.clone()),
            rating: rating_of(part.0, part.1 + 1),
            rateable: all_out && finished,
        });
    }
    // Seasons no AniList entry covers are rated as seasons.
    let mut seasons: Vec<i64> = list.iter().map(|e| e.season).collect();
    seasons.dedup();
    for season in seasons {
        let idx: Vec<usize> = (0..list.len())
            .filter(|&i| list[i].season == season)
            .collect();
        if idx.iter().any(|&i| covered[i]) {
            continue;
        }
        let all_out = idx.iter().all(|&i| list[i].released);
        units.push(RatingUnit {
            season,
            first_episode: 1,
            episodes: idx.len() as i64,
            name: None,
            rating: rating_of(season, 1),
            // A season still airing is not over, whatever is out so far.
            rateable: all_out && next_season != Some(season),
        });
    }
    for (season, first, rating) in &rated {
        if !units
            .iter()
            .any(|u| u.season == *season && u.first_episode == *first)
        {
            units.push(RatingUnit {
                season: *season,
                first_episode: *first,
                episodes: 0,
                name: None,
                rating: Some(*rating),
                rateable: true,
            });
        }
    }
    units.sort_by_key(|u| (u.season, u.first_episode));
    Ok(units)
}

/// Move a show to the status its episodes call for, if it is not there.
pub async fn reconcile(pool: &SqlitePool, user_id: i64, title_id: i64) -> AppResult<()> {
    let Some(entry) = store::get_entry(pool, user_id, title_id).await? else {
        return Ok(());
    };
    let (kind, from_anilist): (String, bool) = sqlx::query_as(
        "SELECT kind, EXISTS (SELECT 1 FROM title_external x
                               WHERE x.title_id = title.id AND x.provider = 'anilist'
                                 AND x.is_primary = 1)
           FROM title WHERE id = ?",
    )
    .bind(title_id)
    .fetch_one(pool)
    .await?;
    if kind != "tv" {
        return Ok(());
    }
    // A title from AniList with no episodes listed has nothing to measure
    // against; its status is AniList's word (see `anilist::local::sync_local`).
    if from_anilist {
        let (episodes,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM episode WHERE title_id = ? AND season_number > 0")
                .bind(title_id)
                .fetch_one(pool)
                .await?;
        if episodes == 0 {
            return Ok(());
        }
    }
    let state = show_state(pool, user_id, title_id).await?;
    let auto = crate::routes::settings::auto_complete(pool, user_id).await;
    if let Some(status) = desired_status(Some(&entry.status), &state, auto) {
        let patch = store::LibraryPatch {
            status: Some(status.to_string()),
            ..Default::default()
        };
        store::apply_patch(pool, user_id, title_id, &patch).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(released: i64, watched: i64, ongoing: bool) -> ShowState {
        ShowState {
            released,
            watched,
            ongoing,
        }
    }

    #[test]
    fn a_finished_season_with_an_announced_sequel_is_completed() {
        assert_eq!(
            desired_status(Some("watching"), &state(12, 12, false), true),
            Some("completed")
        );
    }

    #[test]
    fn caught_up_on_an_airing_run_stays_watching() {
        assert_eq!(
            desired_status(Some("completed"), &state(801, 801, true), true),
            Some("watching")
        );
        assert_eq!(
            desired_status(Some("watching"), &state(801, 801, true), true),
            None
        );
    }

    #[test]
    fn released_episodes_left_means_watching() {
        assert_eq!(
            desired_status(Some("completed"), &state(13, 12, false), true),
            Some("watching")
        );
        assert_eq!(
            desired_status(Some("planned"), &state(13, 1, false), true),
            Some("watching")
        );
    }

    #[test]
    fn nothing_out_is_never_watching() {
        assert_eq!(
            desired_status(Some("watching"), &state(0, 0, false), true),
            Some("planned")
        );
        assert_eq!(
            desired_status(Some("planned"), &state(0, 0, false), true),
            None
        );
    }

    #[test]
    fn paused_dropped_and_untouched_shows_are_left_alone() {
        assert_eq!(
            desired_status(Some("paused"), &state(12, 12, false), true),
            None
        );
        assert_eq!(
            desired_status(Some("dropped"), &state(12, 3, false), true),
            None
        );
        assert_eq!(
            desired_status(Some("planned"), &state(12, 0, false), true),
            None
        );
        assert_eq!(
            desired_status(Some("watching"), &state(12, 12, false), false),
            None
        );
    }

    #[test]
    fn an_entry_without_a_length_runs_to_the_next_one() {
        // Two seasons of 3 and 2 episodes.
        let order = [(1, 1), (1, 2), (1, 3), (2, 1), (2, 2)];
        // One entry of unknown length from the start: the whole show.
        assert_eq!(ranges(&order, &[(1, 0, None)]), vec![(0, 5)]);
        // Followed by a placed season 2: it stops where that starts.
        assert_eq!(
            ranges(&order, &[(1, 0, None), (2, 0, Some(2))]),
            vec![(0, 3), (3, 5)]
        );
        // A known length that TMDB has fewer episodes for is cut at the end.
        assert_eq!(ranges(&order, &[(2, 1, Some(9))]), vec![(4, 5)]);
    }
}
