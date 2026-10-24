//! The calendar and the diary: the same watch data read forwards and back.

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::AppResult;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct Range {
    /// Inclusive yyyy-mm-dd. Defaults to a month either side of today.
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CalendarEntry {
    pub kind: String,
    pub tmdb_id: i64,
    pub title: String,
    pub poster_path: Option<String>,
    pub accent: Option<String>,
    pub date: String,
    pub season_number: Option<i64>,
    pub episode_number: Option<i64>,
    pub episode_name: Option<String>,
    pub watched: bool,
}

/// (kind, tmdb_id, name, poster, accent, date, season, episode, ep name, watched)
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

/// Episodes of anything followed, plus release dates of films on the
/// watchlist, inside a window.
pub async fn calendar(
    _: Auth,
    State(state): State<AppState>,
    Query(range): Query<Range>,
) -> AppResult<Json<Value>> {
    let from = range.from.unwrap_or_else(|| "now', '-45 day".into());
    let to = range.to.unwrap_or_else(|| "now', '+120 day".into());

    // Both bounds are compared as plain yyyy-mm-dd strings, which sorts
    // correctly and keeps the query index-friendly.
    let from = normalise(&from, -45);
    let to = normalise(&to, 120);

    let episodes: Vec<CalendarRow> = sqlx::query_as(
        "SELECT t.kind, t.tmdb_id, t.name, t.poster_path, t.accent_json,
                e.air_date, e.season_number, e.episode_number, e.name,
                (SELECT COUNT(*) FROM watch_event w WHERE w.episode_id = e.id)
           FROM episode e
           JOIN title t ON t.id = e.title_id
           JOIN library_entry l ON l.title_id = t.id
          WHERE l.status IN ('watching', 'planned', 'paused')
            AND e.air_date IS NOT NULL
            AND e.air_date >= ? AND e.air_date <= ?
          ORDER BY e.air_date, t.name, e.season_number, e.episode_number
          LIMIT 600",
    )
    .bind(&from)
    .bind(&to)
    .fetch_all(&state.pool)
    .await?;

    let mut entries: Vec<CalendarEntry> = episodes
        .into_iter()
        .map(
            |(kind, tmdb_id, title, poster, accent, date, season, episode, ep_name, watched)| {
                CalendarEntry {
                    kind,
                    tmdb_id,
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
        "SELECT t.kind, t.tmdb_id, t.name, t.poster_path, t.accent_json, t.release_date
           FROM title t
           JOIN library_entry l ON l.title_id = t.id
          WHERE t.kind = 'movie'
            AND l.status = 'planned'
            AND t.release_date IS NOT NULL
            AND t.release_date >= ? AND t.release_date <= ?
          ORDER BY t.release_date",
    )
    .bind(&from)
    .bind(&to)
    .fetch_all(&state.pool)
    .await?;

    entries.extend(
        films.into_iter().map(
            |(kind, tmdb_id, title, poster, accent, date)| CalendarEntry {
                kind,
                tmdb_id,
                title,
                poster_path: poster,
                accent: accent_of(accent.as_deref()),
                date,
                season_number: None,
                episode_number: None,
                episode_name: None,
                watched: false,
            },
        ),
    );

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

/// (kind, tmdb_id, name, poster, accent, release date)
type FilmRow = (String, i64, String, Option<String>, Option<String>, String);

/// (watched_at, rewatch, source, kind, tmdb_id, name, poster, season, episode, ep name, rating)
type DiaryRow = (
    String,
    i64,
    String,
    String,
    i64,
    String,
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<String>,
    Option<f64>,
);

/// Everything watched, newest first.
pub async fn diary(
    _: Auth,
    State(state): State<AppState>,
    Query(q): Query<DiaryQuery>,
) -> AppResult<Json<Value>> {
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let offset = q.offset.unwrap_or(0).max(0);

    let rows: Vec<DiaryRow> = sqlx::query_as(
        "SELECT w.watched_at, w.is_rewatch, w.source,
                t.kind, t.tmdb_id, t.name, t.poster_path,
                e.season_number, e.episode_number, e.name,
                COALESCE(s.rating, l.rating)
           FROM watch_event w
           JOIN title t ON t.id = w.title_id
           LEFT JOIN episode e ON e.id = w.episode_id
           LEFT JOIN episode_state s ON s.episode_id = e.id
           LEFT JOIN library_entry l ON l.title_id = t.id
          ORDER BY w.watched_at DESC, w.id DESC
          LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let entries: Vec<Value> = rows
        .into_iter()
        .map(
            |(
                watched_at,
                rewatch,
                source,
                kind,
                tmdb_id,
                name,
                poster_path,
                season,
                episode,
                ep_name,
                rating,
            )| {
                json!({
                    "watched_at": watched_at,
                    "rewatch": rewatch == 1,
                    "source": source,
                    "kind": kind,
                    "tmdb_id": tmdb_id,
                    "title": name,
                    "poster_path": poster_path,
                    "season_number": season,
                    "episode_number": episode,
                    "episode_name": ep_name,
                    "rating": rating,
                })
            },
        )
        .collect();

    let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM watch_event")
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(json!({ "entries": entries, "total": total })))
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
