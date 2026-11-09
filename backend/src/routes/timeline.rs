//! The calendar and the diary: the same watch data read forwards and back.

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::AppResult;
use crate::release::FOLLOWED;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct Range {
    /// Inclusive yyyy-mm-dd. Defaults to a month either side of today.
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CalendarEntry {
    /// Frog's id for the title.
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub poster_path: Option<String>,
    pub accent: Option<String>,
    pub date: String,
    pub season_number: Option<i64>,
    pub episode_number: Option<i64>,
    pub episode_name: Option<String>,
    pub watched: bool,
}

/// (kind, title id, name, poster, accent, date, season, episode, ep name, watched)
type CalendarRow = (
    String,
    i64,
    String,
    Option<String>,
    Option<String>,
    String,
    i64,
    i64,
    Option<String>,
    i64,
);

/// (title id, show, AniList entry name, poster, accent, premiere, season, offset)
type PremiereRow = (
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    Option<i64>,
    i64,
);

/// Episodes of anything followed (`release::FOLLOWED`), plus release dates of
/// films on the watchlist, inside a window.
pub async fn calendar(
    auth: Auth,
    State(state): State<AppState>,
    Query(range): Query<Range>,
) -> AppResult<Json<Value>> {
    let from = range.from.unwrap_or_else(|| "now', '-45 day".into());
    let to = range.to.unwrap_or_else(|| "now', '+120 day".into());

    // Both bounds are compared as plain yyyy-mm-dd strings, which sorts
    // correctly and keeps the query index-friendly.
    let from = normalise(&from, -45);
    let to = normalise(&to, 120);

    let episodes: Vec<CalendarRow> = sqlx::query_as(&format!(
        "SELECT t.kind, t.id, t.name, t.poster_path, t.accent_json,
                e.air_date, e.season_number, e.episode_number, e.name,
                (SELECT COUNT(*) FROM watch_event w
                  WHERE w.user_id = ? AND w.episode_id = e.id)
           FROM episode e
           JOIN title t ON t.id = e.title_id
           JOIN library_entry l ON l.title_id = t.id AND l.user_id = ?
          WHERE {FOLLOWED}
            AND e.air_date IS NOT NULL
            AND e.air_date >= ? AND e.air_date <= ?
          ORDER BY e.air_date, t.name, e.season_number, e.episode_number
          LIMIT 600"
    ))
    .bind(auth.user_id())
    .bind(auth.user_id())
    .bind(&from)
    .bind(&to)
    .fetch_all(&state.pool)
    .await?;

    let keys: Vec<(i64, &str, i64)> = episodes.iter().map(|r| (r.1, r.5.as_str(), r.6)).collect();
    let keep = crate::release::calendar_keeps(&keys);
    let mut entries: Vec<CalendarEntry> = episodes
        .into_iter()
        .zip(keep)
        .filter_map(|(row, keep)| keep.then_some(row))
        .map(
            |(kind, id, title, poster, accent, date, season, episode, ep_name, watched)| {
                CalendarEntry {
                    id,
                    kind,
                    title,
                    poster_path: poster,
                    accent: accent_of(accent.as_deref()),
                    date,
                    season_number: Some(season),
                    episode_number: Some(episode),
                    episode_name: ep_name,
                    watched: watched > 0,
                }
            },
        )
        .collect();

    let films: Vec<FilmRow> = sqlx::query_as(
        "SELECT t.kind, t.id, t.name, t.poster_path, t.accent_json, t.release_date
           FROM title t
           JOIN library_entry l ON l.title_id = t.id AND l.user_id = ?
          WHERE t.kind = 'movie'
            AND l.status = 'planned'
            AND t.release_date IS NOT NULL
            AND t.release_date >= ? AND t.release_date <= ?
          ORDER BY t.release_date",
    )
    .bind(auth.user_id())
    .bind(&from)
    .bind(&to)
    .fetch_all(&state.pool)
    .await?;

    entries.extend(
        films
            .into_iter()
            .map(|(kind, id, title, poster, accent, date)| CalendarEntry {
                id,
                kind,
                title,
                poster_path: poster,
                accent: accent_of(accent.as_deref()),
                date,
                season_number: None,
                episode_number: None,
                episode_name: None,
                watched: false,
            }),
    );

    // AniList knows premiere dates TMDB has not listed yet: a season that is
    // not on TMDB at all, or not yet with dates. Only whole dates, and only
    // when no episode of the same show is already on that day.
    let premieres: Vec<PremiereRow> = sqlx::query_as(&format!(
        "SELECT t.id, t.name, a.name, t.poster_path, t.accent_json, a.start_date,
                a.season, a.episode_offset
           FROM anilist_entry a
           JOIN title t ON t.id = a.title_id AND t.kind = 'tv'
           JOIN library_entry l ON l.title_id = t.id AND l.user_id = a.user_id
          WHERE a.user_id = ? AND a.state != 'ignored'
            AND {FOLLOWED}
            AND a.media_status = 'NOT_YET_RELEASED'
            AND a.start_date IS NOT NULL
            AND a.start_date >= ? AND a.start_date <= ?"
    ))
    .bind(auth.user_id())
    .bind(&from)
    .bind(&to)
    .fetch_all(&state.pool)
    .await?;
    for (id, show, season_name, poster, accent, date, season, offset) in premieres {
        let taken = entries.iter().any(|e| e.id == id && e.date == date);
        if taken {
            continue;
        }
        entries.push(CalendarEntry {
            id,
            kind: "tv".into(),
            title: show,
            poster_path: poster,
            accent: accent_of(accent.as_deref()),
            date,
            season_number: season,
            episode_number: season.map(|_| offset + 1),
            episode_name: Some(season_name),
            watched: false,
        });
    }

    entries.sort_by(|a, b| a.date.cmp(&b.date));

    Ok(Json(json!({ "from": from, "to": to, "entries": entries })))
}

fn accent_of(raw: Option<&str>) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(raw?).ok()?;
    parsed.get("accent")?.as_str().map(str::to_string)
}

/// Accept a plain date, or fall back to an offset from today.
fn normalise(value: &str, fallback_days: i64) -> String {
    let looks_like_date = value.len() == 10 && value.as_bytes()[4] == b'-';
    if looks_like_date {
        return value.to_string();
    }
    (chrono::Utc::now() + chrono::Duration::days(fallback_days))
        .format("%Y-%m-%d")
        .to_string()
}

#[derive(Debug, Deserialize)]
pub struct DiaryQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// (kind, title id, name, poster, accent, release date)
type FilmRow = (String, i64, String, Option<String>, Option<String>, String);

/// (day, latest timestamp, kind, title id, name, poster, count, rewatch, imported, film, rating)
type DiaryGroupRow = (
    String,
    String,
    String,
    i64,
    String,
    Option<String>,
    i64,
    i64,
    i64,
    i64,
    Option<f64>,
);

/// (season, episode, name, rating, rewatch)
type DiaryEpisodeRow = (i64, i64, Option<String>, Option<f64>, i64);

/// Everything watched and read, newest first, one entry per title per day.
///
/// Ticking eight episodes in an evening is one thing that happened, not
/// eight, so episodes of the same show on the same day come back as a single
/// group carrying its episodes. Grouping here rather than in the browser keeps
/// pagination honest: a 135-episode catch-up is one entry, never split across
/// pages. Watches recorded without a date are counted but not listed. A
/// manga's day is its chapters read (`chapter_read`), grouped the same way.
pub async fn diary(
    auth: Auth,
    State(state): State<AppState>,
    Query(q): Query<DiaryQuery>,
) -> AppResult<Json<Value>> {
    let limit = q.limit.unwrap_or(60).clamp(1, 300);
    let offset = q.offset.unwrap_or(0).max(0);

    let groups: Vec<DiaryGroupRow> = sqlx::query_as(
        "SELECT * FROM (
            SELECT substr(w.watched_at, 1, 10) AS day,
                   MAX(w.watched_at) AS latest,
                   t.kind, t.id, t.name, t.poster_path,
                   COUNT(*),
                   MAX(w.is_rewatch),
                   MAX(w.source = 'import'),
                   w.episode_id IS NULL AS film,
                   l.rating
              FROM watch_event w
              JOIN title t ON t.id = w.title_id
              -- Only what is in the library: a title taken out leaves the diary.
              JOIN library_entry l ON l.title_id = t.id AND l.user_id = ?1
             WHERE w.user_id = ?1 AND w.undated = 0
             GROUP BY day, t.id, film
            UNION ALL
            SELECT substr(r.read_at, 1, 10) AS day,
                   MAX(r.read_at) AS latest,
                   t.kind, t.id, t.name, t.poster_path,
                   COUNT(*), 0, MAX(r.source = 'import'), 0, l.rating
              FROM chapter_read r
              JOIN title t ON t.id = r.title_id
              JOIN library_entry l ON l.title_id = t.id AND l.user_id = r.user_id
             WHERE r.user_id = ?1
             GROUP BY day, t.id)
          ORDER BY day DESC, latest DESC
          LIMIT ?2 OFFSET ?3",
    )
    .bind(auth.user_id())
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let mut entries = Vec::with_capacity(groups.len());
    for (day, latest, kind, title_id, name, poster_path, count, rewatch, imported, film, rating) in
        groups
    {
        let manga = kind == "manga";
        let chapters: Vec<(i64,)> = if manga {
            sqlx::query_as(
                "SELECT DISTINCT chapter FROM chapter_read
                  WHERE user_id = ? AND title_id = ? AND substr(read_at, 1, 10) = ?
                  ORDER BY chapter",
            )
            .bind(auth.user_id())
            .bind(title_id)
            .bind(&day)
            .fetch_all(&state.pool)
            .await?
        } else {
            Vec::new()
        };
        let episodes: Vec<DiaryEpisodeRow> = if film == 1 || manga {
            Vec::new()
        } else {
            sqlx::query_as(
                "SELECT e.season_number, e.episode_number, e.name, s.rating, w.is_rewatch
                   FROM watch_event w
                   JOIN episode e ON e.id = w.episode_id
                   LEFT JOIN episode_state s ON s.episode_id = e.id AND s.user_id = ?
                  WHERE w.user_id = ?
                    AND w.title_id = ?
                    AND w.undated = 0
                    AND substr(w.watched_at, 1, 10) = ?
                  ORDER BY e.season_number, e.episode_number",
            )
            .bind(auth.user_id())
            .bind(auth.user_id())
            .bind(title_id)
            .bind(&day)
            .fetch_all(&state.pool)
            .await?
        };

        entries.push(json!({
            "day": day,
            "watched_at": latest,
            "id": title_id,
            "kind": kind,
            "title": name,
            "poster_path": poster_path,
            "film": film == 1,
            "count": count,
            "rewatch": rewatch == 1,
            "imported": imported == 1,
            "rating": if film == 1 || manga { rating } else { None },
            "chapters": chapters.into_iter().map(|c| c.0).collect::<Vec<_>>(),
            "episodes": episodes
                .into_iter()
                .map(|(s, e, ep_name, ep_rating, ep_rewatch)| json!({
                    "season_number": s,
                    "episode_number": e,
                    "name": ep_name,
                    "rating": ep_rating,
                    "rewatch": ep_rewatch == 1,
                }))
                .collect::<Vec<_>>(),
        }));
    }

    let (total,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM (
            SELECT 1 FROM watch_event w
             JOIN library_entry l ON l.title_id = w.title_id AND l.user_id = w.user_id
             WHERE w.user_id = ?1 AND w.undated = 0
             GROUP BY substr(w.watched_at, 1, 10), w.title_id, w.episode_id IS NULL
            UNION ALL
            SELECT 1 FROM chapter_read r
             WHERE r.user_id = ?1
             GROUP BY substr(r.read_at, 1, 10), r.title_id)",
    )
    .bind(auth.user_id())
    .fetch_one(&state.pool)
    .await?;

    let (undated,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM watch_event w
               JOIN library_entry l ON l.title_id = w.title_id AND l.user_id = w.user_id
              WHERE w.user_id = ? AND w.undated = 1",
    )
    .bind(auth.user_id())
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(
        json!({ "entries": entries, "total": total, "undated": undated }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct RedateBody {
    /// Frog's id for the title.
    pub title_id: i64,
    /// The diary day the group sits on now, yyyy-mm-dd.
    pub day: String,
    pub when: crate::bulk::When,
    pub date: Option<String>,
}

/// Move a day's group of watches to a different date, onto each episode's
/// release date, or out of the diary altogether. This is how a catch-up that
/// was logged as "today" gets put right after the fact.
pub async fn redate(
    auth: Auth,
    State(state): State<AppState>,
    Json(body): Json<RedateBody>,
) -> AppResult<Json<Value>> {
    use crate::bulk::{evening_of, When};

    let title_id = body.title_id;

    // Numbered parameters, because each branch binds a different number of
    // them: ?1 title, ?2 day, ?3 the account, ?4 a timestamp where one is used.
    let scope = "WHERE user_id = ?3 AND title_id = ?1 AND undated = 0
                   AND substr(watched_at, 1, 10) = ?2";

    let changed = match body.when {
        When::Unknown => {
            sqlx::query(&format!("UPDATE watch_event SET undated = 1 {scope}"))
                .bind(title_id)
                .bind(&body.day)
                .bind(auth.user_id())
                .execute(&state.pool)
                .await?
        }
        When::Release => {
            // Episodes take their air date; a film takes its release date.
            sqlx::query(&format!(
                "UPDATE watch_event
                    SET watched_at = COALESCE(
                            (SELECT e.air_date || 'T20:00:00Z' FROM episode e
                              WHERE e.id = watch_event.episode_id),
                            (SELECT t.release_date || 'T20:00:00Z' FROM title t
                              WHERE t.id = watch_event.title_id AND watch_event.episode_id IS NULL),
                            watched_at),
                        undated = CASE WHEN COALESCE(
                            (SELECT e.air_date FROM episode e WHERE e.id = watch_event.episode_id),
                            (SELECT t.release_date FROM title t
                              WHERE t.id = watch_event.title_id AND watch_event.episode_id IS NULL)
                        ) IS NULL THEN 1 ELSE 0 END
                  {scope}"
            ))
            .bind(title_id)
            .bind(&body.day)
            .bind(auth.user_id())
            .execute(&state.pool)
            .await?
        }
        When::Date => {
            let stamp = evening_of(body.date.as_deref().unwrap_or(""))?;
            sqlx::query(&format!("UPDATE watch_event SET watched_at = ?4 {scope}"))
                .bind(title_id)
                .bind(&body.day)
                .bind(auth.user_id())
                .bind(stamp)
                .execute(&state.pool)
                .await?
        }
        When::Today => {
            sqlx::query(&format!("UPDATE watch_event SET watched_at = ?4 {scope}"))
                .bind(title_id)
                .bind(&body.day)
                .bind(auth.user_id())
                .bind(chrono::Utc::now().to_rfc3339())
                .execute(&state.pool)
                .await?
        }
    };

    // A manga's day of reading moves the same way. Chapters have no release
    // dates, so that choice leaves them where they are; a day not known takes
    // them out of the diary, and the count stays.
    let day_reads = "WHERE user_id = ?1 AND title_id = ?2 AND substr(read_at, 1, 10) = ?3";
    let (sql, stamp) = match body.when {
        When::Release => (None, None),
        When::Unknown => (Some(format!("DELETE FROM chapter_read {day_reads}")), None),
        When::Date => (
            Some(format!("UPDATE chapter_read SET read_at = ?4 {day_reads}")),
            Some(evening_of(body.date.as_deref().unwrap_or(""))?),
        ),
        When::Today => (
            Some(format!("UPDATE chapter_read SET read_at = ?4 {day_reads}")),
            Some(chrono::Utc::now().to_rfc3339()),
        ),
    };
    let read = match sql {
        Some(sql) => {
            let mut q = sqlx::query(&sql)
                .bind(auth.user_id())
                .bind(title_id)
                .bind(&body.day);
            if let Some(stamp) = stamp {
                q = q.bind(stamp);
            }
            q.execute(&state.pool).await?.rows_affected()
        }
        None => 0,
    };

    Ok(Json(json!({ "changed": changed.rows_affected() + read })))
}

#[cfg(test)]
mod tests {
    use super::normalise;

    #[test]
    fn a_real_date_is_kept_and_anything_else_falls_back() {
        assert_eq!(normalise("2026-03-01", 10), "2026-03-01");
        assert_eq!(normalise("nonsense", 0).len(), 10);
        assert_ne!(normalise("", 30), normalise("", -30));
    }
}
