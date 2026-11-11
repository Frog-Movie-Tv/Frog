//! What Stats counts, for one account: the stats page and the year in review it shares, in two halves as AniList
//! keeps its own: films and shows, from what was watched, and manga, from
//! what was read.
//!
//! Every figure is computed over one scope: either everything, or the watches
//! dated within one calendar year. Undated watches (a back catalogue ticked
//! off with no idea when) count towards "all time" but belong to no year.

use chrono::{Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::sqlite::SqliteRow;
use sqlx::{FromRow, SqlitePool};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    pub year: Option<i32>,
}

/// Which watches a figure is computed over: whose they are, and when; and,
/// under a pick on the page, which titles and which month or year.
struct Scope {
    user_id: i64,
    year: Option<String>,
    /// Frog's title ids, already checked to be numbers, joined by commas.
    titles: Option<String>,
    /// A month (YYYY-MM) or year (YYYY), already checked to be one.
    unit: Option<String>,
}

impl Scope {
    /// Every query starts from `scoped`, so the account and the year filter
    /// live in one place. Only titles still in the library count: taking
    /// something out of it takes it out of the numbers too, so the stats always
    /// add up to the library you see.
    ///
    /// The account is written into the SQL rather than bound: these bodies are
    /// assembled by several callers and a placeholder would have to be bound in
    /// an order each of them agreed on. It is an `i64` from the session, so
    /// there is nothing in it to escape. The year is still bound.
    fn sql(&self, body: &str) -> String {
        let mut filter = format!(
            "w.user_id = {0} AND w.title_id IN \
             (SELECT l.title_id FROM library_entry l WHERE l.user_id = {0})",
            self.user_id
        );
        if self.year.is_some() {
            filter.push_str(" AND w.undated = 0 AND substr(w.watched_at, 1, 4) = ?");
        }
        // Both were checked to hold only digits, commas and a dash.
        if let Some(titles) = &self.titles {
            filter.push_str(&format!(" AND w.title_id IN ({titles})"));
        }
        if let Some(unit) = &self.unit {
            filter.push_str(&format!(
                " AND w.undated = 0 AND substr(w.watched_at, 1, {}) = '{unit}'",
                unit.len()
            ));
        }
        format!("WITH scoped AS (SELECT w.* FROM watch_event w WHERE {filter}) {body}")
    }

    /// `library_entry` is not reached through `scoped`, so rows of it carry
    /// this test instead.
    fn mine(&self) -> String {
        format!("l.user_id = {}", self.user_id)
    }

    async fn all<T>(&self, pool: &SqlitePool, body: &str) -> AppResult<Vec<T>>
    where
        T: for<'r> FromRow<'r, SqliteRow> + Send + Unpin,
    {
        let sql = self.sql(body);
        let mut q = sqlx::query_as::<_, T>(&sql);
        if let Some(y) = &self.year {
            q = q.bind(y.clone());
        }
        Ok(q.fetch_all(pool).await?)
    }

    async fn one<T>(&self, pool: &SqlitePool, body: &str) -> AppResult<T>
    where
        T: for<'r> FromRow<'r, SqliteRow> + Send + Unpin,
    {
        let sql = self.sql(body);
        let mut q = sqlx::query_as::<_, T>(&sql);
        if let Some(y) = &self.year {
            q = q.bind(y.clone());
        }
        Ok(q.fetch_one(pool).await?)
    }
}

/// Runtime of one watch: the episode's own when TMDB has it, else the title's.
const MINUTES: &str = "COALESCE(e.runtime, t.runtime, 0)";

/// (films, episodes, shows, titles, rewatches, minutes, active days)
type TotalsRow = (i64, i64, i64, i64, i64, f64, i64);

/// (kind, title id, name, poster, watches, minutes, rating)
type TopRow = (String, i64, String, Option<String>, i64, f64, Option<f64>);

/// (tmdb_id, name, profile or logo, titles)
type PersonRow = (i64, String, Option<String>, i64);

/// (title id, kind, name, poster, genres, release year, rating, unit,
///  minutes, films, episodes, rewatches)
type FactRow = (
    i64,
    String,
    String,
    Option<String>,
    String,
    Option<String>,
    Option<f64>,
    String,
    f64,
    i64,
    i64,
    i64,
);

/// One title as the page filters it: what it is, how you rated it, and what
/// was watched of it in each month (a year) or year (all time). Clicking a bar
/// on the page narrows every other chart to the titles under it, so the page
/// needs the figures per title, not only the sums.
fn facts_json(rows: Vec<FactRow>) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut last: Option<i64> = None;
    for (id, kind, name, poster, genres, year, rating, unit, minutes, films, episodes, again) in
        rows
    {
        if last != Some(id) {
            last = Some(id);
            out.push(json!({
                "id": id,
                "kind": kind,
                "name": name,
                "poster_path": poster,
                "genres": crate::catalog::parse_genres(&genres),
                "year": year.and_then(|y| y.parse::<i64>().ok()),
                "rating": rating,
                "units": [],
            }));
        }
        if films + episodes > 0 {
            if let Some(units) = out.last_mut().and_then(|t| t["units"].as_array_mut()) {
                units.push(json!([
                    unit,
                    minutes.round() as i64,
                    films,
                    episodes,
                    again
                ]));
            }
        }
    }
    out
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Streak {
    pub days: i64,
    pub from: Option<String>,
    pub to: Option<String>,
}

pub async fn stats(state: &AppState, user_id: i64, q: &StatsQuery) -> AppResult<Value> {
    if let Some(y) = q.year {
        if !(1900..=2100).contains(&y) {
            return Err(AppError::BadRequest(
                "year must be between 1900 and 2100".into(),
            ));
        }
    }
    let scope = Scope {
        user_id,
        year: q.year.map(|y| y.to_string()),
        titles: None,
        unit: None,
    };
    let pool = &state.pool;

    let years: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT substr(watched_at, 1, 4) FROM watch_event
          WHERE user_id = ? AND undated = 0 ORDER BY 1 DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let years: Vec<i32> = years
        .into_iter()
        .filter_map(|(y,)| y.parse().ok())
        .collect();

    let (films, episodes, shows, titles, rewatches, minutes, days): TotalsRow = scope
        .one(
            pool,
            &format!(
                "SELECT COALESCE(SUM(s.episode_id IS NULL), 0),
                        COALESCE(SUM(s.episode_id IS NOT NULL), 0),
                        COUNT(DISTINCT CASE WHEN s.episode_id IS NOT NULL THEN s.title_id END),
                        COUNT(DISTINCT s.title_id),
                        COALESCE(SUM(s.is_rewatch), 0),
                        CAST(COALESCE(SUM({MINUTES}), 0) AS REAL),
                        COUNT(DISTINCT CASE WHEN s.undated = 0 THEN substr(s.watched_at, 1, 10) END)
                   FROM scoped s
                   JOIN title t ON t.id = s.title_id
                   LEFT JOIN episode e ON e.id = s.episode_id"
            ),
        )
        .await?;

    // A year is drawn month by month; all time, year by year.
    let unit_len = if scope.year.is_some() { 7 } else { 4 };
    let timeline: Vec<(String, f64, i64, i64)> = scope
        .all(
            pool,
            &format!(
                "SELECT substr(s.watched_at, 1, {unit_len}) AS unit,
                        CAST(COALESCE(SUM({MINUTES}), 0) AS REAL),
                        COALESCE(SUM(s.episode_id IS NULL), 0),
                        COALESCE(SUM(s.episode_id IS NOT NULL), 0)
                   FROM scoped s
                   JOIN title t ON t.id = s.title_id
                   LEFT JOIN episode e ON e.id = s.episode_id
                  WHERE s.undated = 0
                  GROUP BY unit ORDER BY unit"
            ),
        )
        .await?;
    let timeline = fill_timeline(scope.year.as_deref(), timeline);

    let genres: Vec<(String, i64, f64)> = scope
        .all(
            pool,
            &format!(
                "SELECT g.value, COUNT(DISTINCT t.id), CAST(COALESCE(SUM({MINUTES}), 0) AS REAL)
                   FROM scoped s
                   JOIN title t ON t.id = s.title_id
                   LEFT JOIN episode e ON e.id = s.episode_id,
                   json_each(t.genres_json) g
                  GROUP BY g.value
                  ORDER BY 2 DESC, 3 DESC
                  LIMIT 12"
            ),
        )
        .await?;

    let decades: Vec<(i64, i64)> = scope
        .all(
            pool,
            "SELECT (CAST(substr(t.release_date, 1, 4) AS INTEGER) / 10) * 10 AS decade, COUNT(*)
               FROM title t
              WHERE t.id IN (SELECT title_id FROM scoped)
                AND length(t.release_date) >= 4
              GROUP BY decade ORDER BY decade",
        )
        .await?;

    // All time covers every rating, including titles rated without a
    // recorded watch; a year covers what was watched in it. Manga have their
    // own half.
    let rated = if scope.year.is_some() {
        format!(
            "{} AND l.rating IS NOT NULL AND l.title_id IN (SELECT title_id FROM scoped)",
            scope.mine()
        )
    } else {
        format!(
            "{} AND l.rating IS NOT NULL
             AND l.title_id IN (SELECT id FROM title WHERE kind != 'manga')",
            scope.mine()
        )
    };
    let buckets: Vec<(i64, i64)> = scope
        .all(
            pool,
            &format!(
                "SELECT MIN(MAX(CAST(ROUND(l.rating / 10.0) AS INTEGER), 1), 10) - 1 AS bucket, COUNT(*)
                   FROM library_entry l WHERE {rated}
                  GROUP BY bucket"
            ),
        )
        .await?;
    // Bar i is the score (i + 1) out of ten, rounded to the nearest: an 8.5
    // counts as a 9, and a 9 and a 10 stay apart.
    let mut ratings = [0i64; 10];
    for (b, n) in buckets {
        if let Some(slot) = ratings.get_mut(b.clamp(0, 9) as usize) {
            *slot += n;
        }
    }
    let (mean_rating,): (Option<f64>,) = scope
        .one(
            pool,
            &format!("SELECT AVG(l.rating) FROM library_entry l WHERE {rated}"),
        )
        .await?;

    let (longest, current) = scope_streaks(&scope, pool).await?;

    let mine = scope.mine();

    let top_shows: Vec<TopRow> = scope
        .all(
            pool,
            &format!(
                "SELECT t.kind, t.id, t.name, t.poster_path, COUNT(*),
                        CAST(COALESCE(SUM({MINUTES}), 0) AS REAL), l.rating
                   FROM scoped s
                   JOIN title t ON t.id = s.title_id
                   JOIN episode e ON e.id = s.episode_id
                   LEFT JOIN library_entry l ON l.title_id = t.id AND {mine}
                  GROUP BY t.id
                  ORDER BY COUNT(*) DESC, MAX(s.watched_at) DESC
                  LIMIT 6"
            ),
        )
        .await?;

    // Best first: what you rated highest, then what you kept coming back to.
    let top_films: Vec<TopRow> = scope
        .all(
            pool,
            &format!(
                "SELECT t.kind, t.id, t.name, t.poster_path, COUNT(*),
                    CAST(COALESCE(MAX(t.runtime), 0) * COUNT(*) AS REAL), l.rating
               FROM scoped s
               JOIN title t ON t.id = s.title_id
               LEFT JOIN library_entry l ON l.title_id = t.id AND {mine}
              WHERE s.episode_id IS NULL
              GROUP BY t.id
              ORDER BY l.rating IS NULL, l.rating DESC, COUNT(*) DESC, MAX(s.watched_at) DESC
              LIMIT 6"
            ),
        )
        .await?;

    // Every title counted above, watched or only rated, a row per unit.
    let facts: Vec<FactRow> = scope
        .all(
            pool,
            &format!(
                "SELECT t.id, t.kind, t.name, t.poster_path, t.genres_json,
                        substr(t.release_date, 1, 4), l.rating,
                        CASE WHEN s.undated = 0 THEN substr(s.watched_at, 1, {unit_len}) ELSE '' END
                          AS unit,
                        CAST(COALESCE(SUM({MINUTES}), 0) AS REAL),
                        COALESCE(SUM(s.episode_id IS NULL), 0),
                        COALESCE(SUM(s.episode_id IS NOT NULL), 0),
                        COALESCE(SUM(s.is_rewatch), 0)
                   FROM scoped s
                   JOIN title t ON t.id = s.title_id
                   LEFT JOIN episode e ON e.id = s.episode_id
                   LEFT JOIN library_entry l ON l.title_id = t.id AND {mine}
                  GROUP BY t.id, unit
                 UNION ALL
                 SELECT t.id, t.kind, t.name, t.poster_path, t.genres_json,
                        substr(t.release_date, 1, 4), l.rating, '', 0.0, 0, 0, 0
                   FROM library_entry l
                   JOIN title t ON t.id = l.title_id
                  WHERE {rated} AND l.title_id NOT IN (SELECT title_id FROM scoped)
                  ORDER BY 1, 8"
            ),
        )
        .await?;

    let actors = people(&scope, pool, "'cast'").await?;
    let directors = people(&scope, pool, "'director', 'creator'").await?;
    let studios = people(&scope, pool, "'studio'").await?;

    // Titles whose credits have not been fetched yet, so the page can say the
    // people rankings are still filling in.
    let (credits_pending,): (i64,) = scope
        .one(
            pool,
            "SELECT COUNT(*) FROM title t
              WHERE t.people_synced_at IS NULL
                AND t.id IN (SELECT title_id FROM scoped)",
        )
        .await?;

    // The library as it stands, films and shows: what the bar at the end shows.
    let library: Vec<(String, i64)> = sqlx::query_as(
        "SELECT l.status, COUNT(*) FROM library_entry l JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ? AND t.kind != 'manga'
          GROUP BY l.status",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let (manga,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM library_entry l JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ? AND t.kind = 'manga'",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    Ok(json!({
        "year": q.year,
        "years": years,
        "library": crate::store::to_map(library),
        // Whether there is a manga half to offer.
        "manga": manga,
        "totals": {
            "films": films,
            "episodes": episodes,
            "shows": shows,
            "titles": titles,
            "rewatches": rewatches,
            "minutes": minutes.round() as i64,
            "days": days,
            "mean_rating": mean_rating,
        },
        "timeline": timeline,
        "genres": genres.into_iter().map(|(name, titles, minutes)| json!({
            "name": name, "titles": titles, "minutes": minutes.round() as i64,
        })).collect::<Vec<_>>(),
        "decades": decades.into_iter().map(|(decade, titles)| json!({
            "decade": decade, "titles": titles,
        })).collect::<Vec<_>>(),
        "ratings": ratings,
        "streaks": { "longest": longest, "current": current },
        "top_shows": top_shows.into_iter().map(top_json).collect::<Vec<_>>(),
        "top_films": top_films.into_iter().map(top_json).collect::<Vec<_>>(),
        "people": { "actors": actors, "directors": directors, "studios": studios },
        "credits_pending": if state.tmdb.is_configured() { credits_pending } else { 0 },
        "facts": facts_json(facts),
    }))
}

/// (title id, name, poster, chapters read, rating)
type MangaTopRow = (i64, String, Option<String>, i64, Option<f64>);

/// (manga, chapters, completed, days reading, mean rating, rated)
type MangaTotalsRow = (i64, i64, i64, i64, Option<f64>, i64);

/// (title id, name, poster, genres json, status, rating, chapters read so far)
type MangaFactRow = (
    i64,
    String,
    Option<String>,
    Option<String>,
    String,
    Option<f64>,
    i64,
);

/// The manga half of Stats: the manga in the library and the chapters read
/// (`chapter_read`), all time or in one year. All time counts every chapter
/// read, dated or not (`library_entry.progress`); a year counts the chapters
/// read in it, and the manga read in it.
pub async fn manga(state: &AppState, user_id: i64, q: &StatsQuery) -> AppResult<Value> {
    if q.year.is_some_and(|y| !(1900..=2100).contains(&y)) {
        return Err(AppError::BadRequest(
            "year must be between 1900 and 2100".into(),
        ));
    }
    let pool = &state.pool;
    let me = user_id;
    let year = q.year.map(|y| y.to_string()).unwrap_or_default();
    // Every manga in the library, or those read in the year. ?1 is the
    // account and ?2 the year ('' for all time) in every query below.
    let titles = "SELECT l.title_id FROM library_entry l JOIN title t ON t.id = l.title_id
                   WHERE l.user_id = ?1 AND t.kind = 'manga'
                     AND (?2 = '' OR l.title_id IN (SELECT r.title_id FROM chapter_read r
                           WHERE r.user_id = ?1 AND substr(r.read_at, 1, 4) = ?2))";

    let years: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT substr(read_at, 1, 4) FROM chapter_read
          WHERE user_id = ? ORDER BY 1 DESC",
    )
    .bind(me)
    .fetch_all(pool)
    .await?;
    let years: Vec<i32> = years
        .into_iter()
        .filter_map(|(y,)| y.parse().ok())
        .collect();

    let (manga, chapters, completed, days, mean_rating, rated): MangaTotalsRow =
        sqlx::query_as(&format!(
            "SELECT (SELECT COUNT(*) FROM ({titles})),
                    CASE WHEN ?2 = '' THEN
                         (SELECT COALESCE(SUM(l.progress), 0) FROM library_entry l
                           WHERE l.user_id = ?1 AND l.title_id IN ({titles}))
                    ELSE (SELECT COUNT(*) FROM chapter_read r
                           WHERE r.user_id = ?1 AND substr(r.read_at, 1, 4) = ?2
                             AND r.title_id IN ({titles})) END,
                    (SELECT COUNT(*) FROM library_entry l
                      WHERE l.user_id = ?1 AND l.status = 'completed'
                        AND l.title_id IN ({titles})),
                    (SELECT COUNT(DISTINCT substr(r.read_at, 1, 10)) FROM chapter_read r
                      WHERE r.user_id = ?1 AND (?2 = '' OR substr(r.read_at, 1, 4) = ?2)
                        AND r.title_id IN ({titles})),
                    (SELECT AVG(l.rating) FROM library_entry l
                      WHERE l.user_id = ?1 AND l.rating IS NOT NULL
                        AND l.title_id IN ({titles})),
                    (SELECT COUNT(*) FROM library_entry l
                      WHERE l.user_id = ?1 AND l.rating IS NOT NULL
                        AND l.title_id IN ({titles}))"
        ))
        .bind(me)
        .bind(&year)
        .fetch_one(pool)
        .await?;

    // Chapters read by month in a year, by year over all time.
    let unit_len = if q.year.is_some() { 7 } else { 4 };
    let read: Vec<(String, i64)> = sqlx::query_as(&format!(
        "SELECT substr(r.read_at, 1, {unit_len}) AS unit, COUNT(*) FROM chapter_read r
          WHERE r.user_id = ?1 AND (?2 = '' OR substr(r.read_at, 1, 4) = ?2)
            AND r.title_id IN ({titles})
          GROUP BY unit ORDER BY unit"
    ))
    .bind(me)
    .bind(&year)
    .fetch_all(pool)
    .await?;
    let timeline: Vec<Value> = match q.year {
        Some(y) => (1..=12)
            .map(|m| {
                let unit = format!("{y}-{m:02}");
                let n = read.iter().find(|r| r.0 == unit).map_or(0, |r| r.1);
                json!({ "unit": unit, "chapters": n })
            })
            .collect(),
        None => read
            .into_iter()
            .map(|(unit, n)| json!({ "unit": unit, "chapters": n }))
            .collect(),
    };

    let by_status: Vec<(String, i64)> = sqlx::query_as(&format!(
        "SELECT l.status, COUNT(*) FROM library_entry l
          WHERE l.user_id = ?1 AND l.title_id IN ({titles})
          GROUP BY l.status"
    ))
    .bind(me)
    .bind(&year)
    .fetch_all(pool)
    .await?;

    let genres: Vec<(String, i64)> = sqlx::query_as(&format!(
        "SELECT g.value, COUNT(*) FROM title t, json_each(t.genres_json) g
          WHERE t.id IN ({titles})
          GROUP BY g.value ORDER BY 2 DESC, 1 LIMIT 12"
    ))
    .bind(me)
    .bind(&year)
    .fetch_all(pool)
    .await?;

    // Whoever wrote or drew them: a manga's creators.
    let authors: Vec<PersonRow> = sqlx::query_as(&format!(
        "SELECT p.tmdb_id, MAX(p.name), MAX(p.profile_path), COUNT(DISTINCT p.title_id) AS n
           FROM title_person p
          WHERE p.role = 'creator' AND p.title_id IN ({titles})
          GROUP BY p.tmdb_id
          ORDER BY n DESC, MIN(p.ord), MAX(p.name)
          LIMIT 10"
    ))
    .bind(me)
    .bind(&year)
    .fetch_all(pool)
    .await?;

    let buckets: Vec<(i64, i64)> = sqlx::query_as(&format!(
        "SELECT MIN(MAX(CAST(ROUND(l.rating / 10.0) AS INTEGER), 1), 10) - 1 AS bucket, COUNT(*)
           FROM library_entry l
          WHERE l.user_id = ?1 AND l.rating IS NOT NULL AND l.title_id IN ({titles})
          GROUP BY bucket"
    ))
    .bind(me)
    .bind(&year)
    .fetch_all(pool)
    .await?;
    let mut ratings = [0i64; 10];
    for (b, n) in buckets {
        if let Some(slot) = ratings.get_mut(b.clamp(0, 9) as usize) {
            *slot += n;
        }
    }

    // Best first, then what was read furthest.
    let top: Vec<MangaTopRow> = sqlx::query_as(&format!(
        "SELECT t.id, t.name, t.poster_path, COALESCE(l.progress, 0), l.rating
           FROM library_entry l JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ?1 AND l.title_id IN ({titles})
          ORDER BY l.rating IS NULL, l.rating DESC, COALESCE(l.progress, 0) DESC
          LIMIT 12"
    ))
    .bind(me)
    .bind(&year)
    .fetch_all(pool)
    .await?;

    // One row per manga with its reads by month or year, so the page can
    // narrow every chart to the bar someone picks (`lib/stats-filter.ts`).
    let fact_rows: Vec<MangaFactRow> = sqlx::query_as(&format!(
        "SELECT t.id, t.name, t.poster_path, t.genres_json, l.status, l.rating,
                COALESCE(l.progress, 0)
           FROM library_entry l JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ?1 AND l.title_id IN ({titles})"
    ))
    .bind(me)
    .bind(&year)
    .fetch_all(pool)
    .await?;
    let reads: Vec<(i64, String, i64)> = sqlx::query_as(&format!(
        "SELECT r.title_id, substr(r.read_at, 1, {unit_len}) AS unit, COUNT(*) FROM chapter_read r
          WHERE r.user_id = ?1 AND (?2 = '' OR substr(r.read_at, 1, 4) = ?2)
            AND r.title_id IN ({titles})
          GROUP BY r.title_id, unit"
    ))
    .bind(me)
    .bind(&year)
    .fetch_all(pool)
    .await?;
    let facts: Vec<Value> = fact_rows
        .into_iter()
        .map(
            |(id, name, poster_path, genres, status, rating, progress)| {
                let units: Vec<Value> = reads
                    .iter()
                    .filter(|r| r.0 == id)
                    .map(|(_, unit, n)| json!([unit, n]))
                    .collect();
                json!({
                    "id": id, "name": name, "poster_path": poster_path,
                    "genres": crate::catalog::parse_genres(genres.as_deref().unwrap_or("[]")),
                    "status": status, "rating": rating, "progress": progress, "units": units,
                })
            },
        )
        .collect();

    Ok(json!({
        "year": q.year,
        "years": years,
        "facts": facts,
        "totals": {
            "manga": manga,
            "chapters": chapters,
            "completed": completed,
            "days": days,
            "mean_rating": mean_rating,
            "rated": rated,
        },
        "timeline": timeline,
        "by_status": crate::store::to_map(by_status),
        "genres": genres
            .into_iter()
            .map(|(name, titles)| json!({ "name": name, "titles": titles }))
            .collect::<Vec<_>>(),
        "authors": authors
            .into_iter()
            .map(|(id, name, image, titles)| json!({
                "tmdb_id": id, "name": name, "image": image, "titles": titles,
            }))
            .collect::<Vec<_>>(),
        "ratings": ratings,
        "top": top
            .into_iter()
            .map(|(id, name, poster_path, chapters, rating)| json!({
                "id": id, "kind": "manga", "name": name, "poster_path": poster_path,
                "chapters": chapters, "rating": rating,
            }))
            .collect::<Vec<_>>(),
    }))
}

/// The longest and current streak of days with a watch, in a scope.
async fn scope_streaks(scope: &Scope, pool: &SqlitePool) -> AppResult<(Streak, Streak)> {
    let watch_days: Vec<(String,)> = scope
        .all(
            pool,
            "SELECT DISTINCT substr(watched_at, 1, 10) FROM scoped
              WHERE undated = 0 ORDER BY 1",
        )
        .await?;
    let watch_days: Vec<NaiveDate> = watch_days
        .into_iter()
        .filter_map(|(d,)| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
        .collect();
    Ok(streaks(&watch_days, Utc::now().date_naive()))
}

#[derive(Debug, Deserialize)]
pub struct NarrowQuery {
    /// `manga` for the reading half; anything else is films and shows.
    pub half: Option<String>,
    pub year: Option<i32>,
    /// The month (YYYY-MM) or year (YYYY) picked on the page, if one is.
    pub unit: Option<String>,
    /// The titles the page's picks keep, by Frog's id, comma separated.
    pub ids: String,
}

/// Titles a pick can keep at once; a library is not bigger.
const NARROW_MOST: usize = 5000;

/// The ids as SQL can take them: numbers only, or a refusal.
fn id_list(raw: &str) -> AppResult<String> {
    let ids: Vec<i64> = raw
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().parse::<i64>())
        .collect::<Result<_, _>>()
        .map_err(|_| AppError::BadRequest("ids are numbers".into()))?;
    if ids.len() > NARROW_MOST {
        return Err(AppError::BadRequest("too many titles".into()));
    }
    // An empty pick keeps nothing; 0 is no title's id.
    Ok(if ids.is_empty() {
        "0".into()
    } else {
        ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
    })
}

/// A month or year as the page names one.
fn unit_of(raw: Option<&str>) -> AppResult<Option<String>> {
    let Some(unit) = raw.filter(|u| !u.is_empty()) else {
        return Ok(None);
    };
    let shape = unit.len() == 4 || (unit.len() == 7 && unit.as_bytes()[4] == b'-');
    let digits = unit
        .chars()
        .enumerate()
        .all(|(i, c)| i == 4 || c.is_ascii_digit());
    if !(shape && digits) {
        return Err(AppError::BadRequest("unit is YYYY or YYYY-MM".into()));
    }
    Ok(Some(unit.to_string()))
}

/// What the page cannot work out from the facts it holds, for the titles a
/// pick keeps: the days and streaks, and the people. Films and shows give
/// active days, streaks, actors, directors and studios; manga gives days
/// reading and authors.
pub async fn narrow(state: &AppState, user_id: i64, q: &NarrowQuery) -> AppResult<Value> {
    if q.year.is_some_and(|y| !(1900..=2100).contains(&y)) {
        return Err(AppError::BadRequest(
            "year must be between 1900 and 2100".into(),
        ));
    }
    let titles = id_list(&q.ids)?;
    let unit = unit_of(q.unit.as_deref())?;
    let pool = &state.pool;

    if q.half.as_deref() == Some("manga") {
        let me = user_id;
        let year = q.year.map(|y| y.to_string()).unwrap_or_default();
        let unit = unit.unwrap_or_default();
        let (days,): (i64,) = sqlx::query_as(&format!(
            "SELECT COUNT(DISTINCT substr(r.read_at, 1, 10)) FROM chapter_read r
              WHERE r.user_id = ?1 AND (?2 = '' OR substr(r.read_at, 1, 4) = ?2)
                AND (?3 = '' OR substr(r.read_at, 1, length(?3)) = ?3)
                AND r.title_id IN ({titles})"
        ))
        .bind(me)
        .bind(&year)
        .bind(&unit)
        .fetch_one(pool)
        .await?;
        let authors: Vec<PersonRow> = sqlx::query_as(&format!(
            "SELECT p.tmdb_id, MAX(p.name), MAX(p.profile_path), COUNT(DISTINCT p.title_id) AS n
               FROM title_person p
              WHERE p.role = 'creator'
                AND p.title_id IN (SELECT l.title_id FROM library_entry l
                                    WHERE l.user_id = ?1 AND l.title_id IN ({titles}))
              GROUP BY p.tmdb_id
              ORDER BY n DESC, MIN(p.ord), MAX(p.name)
              LIMIT 10"
        ))
        .bind(me)
        .fetch_all(pool)
        .await?;
        return Ok(json!({
            "days": days,
            "authors": authors.into_iter().map(person_json).collect::<Vec<_>>(),
        }));
    }

    let scope = Scope {
        user_id,
        year: q.year.map(|y| y.to_string()),
        titles: Some(titles),
        unit,
    };
    let (days,): (i64,) = scope
        .one(
            pool,
            "SELECT COUNT(DISTINCT substr(watched_at, 1, 10)) FROM scoped WHERE undated = 0",
        )
        .await?;
    let (longest, current) = scope_streaks(&scope, pool).await?;
    Ok(json!({
        "days": days,
        "streaks": { "longest": longest, "current": current },
        "people": {
            "actors": people(&scope, pool, "'cast'").await?,
            "directors": people(&scope, pool, "'director', 'creator'").await?,
            "studios": people(&scope, pool, "'studio'").await?,
        },
    }))
}

fn person_json((tmdb_id, name, image, titles): PersonRow) -> Value {
    json!({ "tmdb_id": tmdb_id, "name": name, "image": image, "titles": titles })
}

async fn people(scope: &Scope, pool: &SqlitePool, roles: &str) -> AppResult<Vec<Value>> {
    // Ties go to whoever is billed highest, so a lead outranks a cameo.
    let rows: Vec<PersonRow> = scope
        .all(
            pool,
            &format!(
                "SELECT p.tmdb_id, MAX(p.name), MAX(p.profile_path), COUNT(DISTINCT p.title_id) AS n
                   FROM title_person p
                  WHERE p.role IN ({roles})
                    AND p.title_id IN (SELECT title_id FROM scoped)
                  GROUP BY p.tmdb_id
                  ORDER BY n DESC, MIN(p.ord) ASC, MAX(p.name)
                  LIMIT 10"
            ),
        )
        .await?;
    Ok(rows.into_iter().map(person_json).collect())
}

fn top_json((kind, id, name, poster_path, watches, minutes, rating): TopRow) -> Value {
    json!({
        "id": id,
        "kind": kind,
        "name": name,
        "poster_path": poster_path,
        "watches": watches,
        "minutes": minutes.round() as i64,
        "rating": rating,
    })
}

/// A year always shows all twelve months, so a quiet month reads as zero
/// rather than vanishing from the chart.
fn fill_timeline(year: Option<&str>, rows: Vec<(String, f64, i64, i64)>) -> Vec<Value> {
    let point = |unit: &str, minutes: f64, films: i64, episodes: i64| json!({ "unit": unit, "minutes": minutes.round() as i64, "films": films, "episodes": episodes });
    let Some(year) = year else {
        return rows
            .iter()
            .map(|(u, m, f, e)| point(u, *m, *f, *e))
            .collect();
    };
    (1..=12)
        .map(|month| {
            let unit = format!("{year}-{month:02}");
            match rows.iter().find(|r| r.0 == unit) {
                Some((_, m, f, e)) => point(&unit, *m, *f, *e),
                None => point(&unit, 0.0, 0, 0),
            }
        })
        .collect()
}

/// Longest run of consecutive days with a watch, and the run still going
/// (one that ended yesterday counts, since today is not over).
pub fn streaks(days: &[NaiveDate], today: NaiveDate) -> (Streak, Streak) {
    let none = || Streak {
        days: 0,
        from: None,
        to: None,
    };
    let mut longest = none();
    let mut current = none();
    let mut run_start: Option<NaiveDate> = None;
    let mut prev: Option<NaiveDate> = None;

    let close = |start: NaiveDate, end: NaiveDate, longest: &mut Streak| {
        let len = (end - start).num_days() + 1;
        if len > longest.days {
            *longest = Streak {
                days: len,
                from: Some(start.to_string()),
                to: Some(end.to_string()),
            };
        }
    };

    for &d in days {
        match (run_start, prev) {
            (Some(_), Some(p)) if d - p == Duration::days(1) => {}
            (Some(start), Some(p)) => {
                close(start, p, &mut longest);
                run_start = Some(d);
            }
            _ => run_start = Some(d),
        }
        prev = Some(d);
    }
    if let (Some(start), Some(end)) = (run_start, prev) {
        close(start, end, &mut longest);
        if today - end <= Duration::days(1) {
            current = Streak {
                days: (end - start).num_days() + 1,
                from: Some(start.to_string()),
                to: Some(end.to_string()),
            };
        }
    }
    (longest, current)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn streaks_find_the_longest_run_and_the_live_one() {
        let days = [
            d("2026-01-01"),
            d("2026-01-02"),
            d("2026-01-03"),
            d("2026-02-10"),
            d("2026-10-25"),
            d("2026-10-26"),
        ];
        let (longest, current) = streaks(&days, d("2026-10-26"));
        assert_eq!(longest.days, 3);
        assert_eq!(longest.from.as_deref(), Some("2026-01-01"));
        assert_eq!(current.days, 2);

        // Yesterday still counts; two days ago does not.
        assert_eq!(streaks(&days, d("2026-10-27")).1.days, 2);
        assert_eq!(streaks(&days, d("2026-10-28")).1.days, 0);
        assert_eq!(streaks(&[], d("2026-10-28")).0.days, 0);
    }

    #[test]
    fn a_year_timeline_always_has_twelve_months() {
        let rows = vec![("2025-03".to_string(), 120.0, 1, 0)];
        let t = fill_timeline(Some("2025"), rows);
        assert_eq!(t.len(), 12);
        assert_eq!(t[2]["minutes"], json!(120));
        assert_eq!(t[11]["unit"], json!("2025-12"));
    }
}
