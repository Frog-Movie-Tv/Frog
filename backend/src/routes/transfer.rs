//! Moving a library in and out of Frog.
//!
//! Parsing is deliberately the browser's job: a Letterboxd ZIP or an IMDb CSV
//! is read there and arrives here as a normalised list, which keeps the
//! wizard responsive and this module free of file-format plumbing. What the
//! server owns is the hard part — deciding which TMDB title each row means,
//! and writing the result without trampling anything.

use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue};
use axum::response::IntoResponse;
use axum::Json;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::store::{self, LibraryPatch, STATUSES};
use crate::tmdb::models::SearchItem;

/// A row as it came out of someone else's export.
#[derive(Debug, Clone, Deserialize)]
pub struct Incoming {
    /// Stable id for the wizard to correlate previews with choices.
    pub key: String,
    pub title: String,
    pub year: Option<i32>,
    pub imdb_id: Option<String>,
    pub tmdb_id: Option<i64>,
    pub kind: Option<String>,
    /// 0..100 after the client has rescaled whatever the source used.
    pub rating: Option<f64>,
    pub status: Option<String>,
    pub watched_at: Option<String>,
    #[serde(default)]
    pub favourite: bool,
    #[serde(default)]
    pub rewatch: bool,
    /// Anime trackers list each season as its own entry; TMDB has one show
    /// with numbered seasons. This is the TMDB season the row is about.
    pub season: Option<i64>,
    /// Episodes watched within that season.
    pub progress: Option<i64>,
    #[serde(default)]
    pub episode_offset: i64,
    pub episode_total: Option<i64>,
    /// Other names to try when the main title finds nothing (English,
    /// romaji, synonyms).
    #[serde(default)]
    pub alt_titles: Vec<String>,
    /// From an anime tracker: prefer animated results when matching.
    #[serde(default)]
    pub anime: bool,
}

#[derive(Debug, Serialize)]
pub struct MatchResult {
    pub key: String,
    pub matched: Option<SearchItem>,
    pub candidates: Vec<SearchItem>,
    /// Why this one needs a human: nothing found, or two plausible answers.
    pub note: Option<String>,
    pub already_tracked: Option<String>,
    /// The season the row maps to, once checked against TMDB.
    pub season: Option<i64>,
    /// The show's highest numbered season on TMDB, so the client can carry
    /// episode numbering on when TMDB folds later seasons into an earlier one.
    pub last_season: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct MatchRequest {
    pub items: Vec<Incoming>,
}

/// Matching is capped so one request cannot sit on TMDB for ten minutes; the
/// wizard sends a few hundred rows at a time and shows progress.
const MAX_BATCH: usize = 200;

pub async fn match_titles(
    _: Auth,
    State(state): State<AppState>,
    Json(body): Json<MatchRequest>,
) -> AppResult<Json<Value>> {
    if body.items.len() > MAX_BATCH {
        return Err(AppError::BadRequest(format!(
            "send at most {MAX_BATCH} rows at a time"
        )));
    }

    let tracked: Vec<(String, i64, String)> = sqlx::query_as(
        "SELECT t.kind, t.tmdb_id, l.status
           FROM library_entry l JOIN title t ON t.id = l.title_id",
    )
    .fetch_all(&state.pool)
    .await?;

    let mut out = Vec::with_capacity(body.items.len());
    for row in body.items {
        let (matched, candidates, mut note) = resolve(&state, &row).await;
        let mut last_season = None;

        // A season the show does not have is the anime case that needs a
        // human: some shows are split into seasons differently on TMDB.
        if let (Some(m), Some(season)) = (matched.as_ref(), row.season) {
            if m.kind == "tv" {
                if let Ok(detail) = state.tmdb.tv(m.tmdb_id).await {
                    let seasons = crate::tmdb::models::seasons(&detail);
                    last_season = seasons.iter().map(|s| s.season_number).max();
                    let known = seasons.iter().any(|s| s.season_number == season);
                    if !known {
                        note = Some(format!(
                            "TMDB lists no season {season} for this show; check the match"
                        ));
                    }
                }
            }
        }

        let already = matched.as_ref().and_then(|m| {
            tracked
                .iter()
                .find(|(kind, id, _)| *kind == m.kind && *id == m.tmdb_id)
                .map(|(_, _, status)| status.clone())
        });

        out.push(MatchResult {
            key: row.key,
            matched,
            candidates,
            note,
            already_tracked: already,
            season: row.season,
            last_season,
        });
    }

    Ok(Json(json!({ "results": out })))
}

/// Work out which TMDB title a row means.
///
/// An IMDb id is unambiguous, so it wins. Otherwise search by name and prefer
/// an exact title in the right year: a year match is what separates the 1978
/// film from the 2008 remake.
async fn resolve(
    state: &AppState,
    row: &Incoming,
) -> (Option<SearchItem>, Vec<SearchItem>, Option<String>) {
    if let (Some(tmdb_id), Some(kind)) = (row.tmdb_id, row.kind.as_deref()) {
        if kind == "movie" || kind == "tv" {
            if let Ok(detail) = fetch_known(state, kind, tmdb_id).await {
                return (Some(detail), Vec::new(), None);
            }
        }
    }

    if let Some(imdb) = row.imdb_id.as_deref().filter(|s| s.starts_with("tt")) {
        if let Some(found) = find_by_imdb(state, imdb).await {
            return (Some(found), Vec::new(), None);
        }
    }

    // English, romaji and synonyms often differ; the first name that finds
    // anything of the right kind wins.
    let names: Vec<&str> = std::iter::once(row.title.as_str())
        .chain(row.alt_titles.iter().map(String::as_str))
        .filter(|n| !n.trim().is_empty())
        .collect();

    let mut found: Vec<SearchItem> = Vec::new();
    let mut searched = row.title.clone();
    for name in names {
        let raw = match state.tmdb.search_multi(name, 1).await {
            Ok(v) => v,
            Err(_) => return (None, Vec::new(), Some("TMDB did not answer".into())),
        };
        found = raw
            .get("results")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| SearchItem::from_value(v, None))
                    .collect()
            })
            .unwrap_or_default();

        if let Some(kind) = row.kind.as_deref() {
            if kind == "movie" || kind == "tv" {
                found.retain(|f| f.kind == kind);
            }
        }
        if !found.is_empty() {
            searched = name.to_string();
            break;
        }
    }

    if found.is_empty() {
        return (None, Vec::new(), Some("No match found".into()));
    }

    // A live-action remake shares a name with half of anime; for rows from
    // an anime tracker, animated results go first.
    if row.anime {
        const ANIMATION: i64 = 16;
        found.sort_by_key(|f| !f.genre_ids.contains(&ANIMATION));
    }

    let wanted = searched.trim().to_lowercase();
    let exact_same_year = found
        .iter()
        .position(|f| f.name.to_lowercase() == wanted && row.year.is_some() && f.year == row.year);

    if let Some(i) = exact_same_year {
        let best = found.remove(i);
        return (Some(best), found.into_iter().take(4).collect(), None);
    }

    let exact = found.iter().position(|f| f.name.to_lowercase() == wanted);
    let index = exact.unwrap_or(0);
    let best = found.remove(index);

    // Say so when the year does not line up: that is the case a person
    // actually needs to look at.
    let later_season = row.season.is_some_and(|s| s > 1);
    let note = match (row.year, best.year) {
        (Some(a), Some(b)) if (a - b).abs() > 1 && !later_season => {
            Some(format!("Your file says {a}, TMDB says {b}"))
        }
        _ if exact.is_none() => Some("Closest match by name".into()),
        _ => None,
    };

    (Some(best), found.into_iter().take(4).collect(), note)
}

async fn fetch_known(state: &AppState, kind: &str, tmdb_id: i64) -> AppResult<SearchItem> {
    let detail = match kind {
        "movie" => state.tmdb.movie(tmdb_id).await?,
        _ => state.tmdb.tv(tmdb_id).await?,
    };
    SearchItem::from_value(&detail, Some(kind))
        .ok_or_else(|| AppError::NotFound("not on TMDB".into()))
}

async fn find_by_imdb(state: &AppState, imdb_id: &str) -> Option<SearchItem> {
    let raw = state
        .tmdb
        .get(
            &format!("/find/{imdb_id}"),
            &[("external_source", "imdb_id".to_string())],
            Duration::days(30),
        )
        .await
        .ok()?;

    for (field, kind) in [("movie_results", "movie"), ("tv_results", "tv")] {
        if let Some(first) = raw
            .get(field)
            .and_then(Value::as_array)
            .and_then(|a| a.first())
        {
            if let Some(item) = SearchItem::from_value(first, Some(kind)) {
                return Some(item);
            }
        }
    }
    None
}

#[derive(Debug, Deserialize)]
pub struct ApplyRequest {
    pub items: Vec<ApplyItem>,
    /// What to do about titles already in the library.
    #[serde(default)]
    pub on_conflict: Conflict,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Conflict {
    /// Leave anything already tracked exactly as it is.
    #[default]
    Skip,
    /// Let the file win.
    Overwrite,
}

#[derive(Debug, Deserialize)]
pub struct ApplyItem {
    pub kind: String,
    pub tmdb_id: i64,
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
}

pub async fn apply(
    _: Auth,
    State(state): State<AppState>,
    Json(body): Json<ApplyRequest>,
) -> AppResult<Json<Value>> {
    if body.items.len() > MAX_BATCH {
        return Err(AppError::BadRequest(format!(
            "import at most {MAX_BATCH} rows at a time"
        )));
    }

    let mut added = 0;
    let mut updated = 0;
    let mut skipped = 0;
    let mut failed: Vec<Value> = Vec::new();

    for item in body.items {
        if item.kind != "movie" && item.kind != "tv" {
            failed.push(json!({ "tmdb_id": item.tmdb_id, "error": "unknown kind" }));
            continue;
        }

        let existing = store::find_title_id(&state.pool, &item.kind, item.tmdb_id).await?;
        let already = match existing {
            Some(id) => store::get_entry(&state.pool, id).await?.is_some(),
            None => false,
        };

        if already && body.on_conflict == Conflict::Skip {
            skipped += 1;
            continue;
        }

        let (title_id, detail) = match store::ensure_title(&state, &item.kind, item.tmdb_id).await {
            Ok(pair) => pair,
            Err(e) => {
                failed.push(json!({ "tmdb_id": item.tmdb_id, "error": e.to_string() }));
                continue;
            }
        };

        let status = item
            .status
            .filter(|s| STATUSES.contains(&s.as_str()))
            .unwrap_or_else(|| "completed".to_string());

        // A later season you only plan to watch must not push a show you are
        // part-way through back onto the watchlist.
        let status = if status == "planned" {
            let watched = store::progress(&state.pool, title_id).await?.watched;
            if watched > 0 {
                "watching".to_string()
            } else {
                status
            }
        } else {
            status
        };

        let completed = status == "completed";
        let patch = LibraryPatch {
            status: Some(status),
            rating: Some(item.rating),
            favourite: Some(item.favourite),
            notes: None,
        };
        store::apply_patch(&state.pool, title_id, &patch).await?;

        if item.kind == "tv" {
            if let Some(season) = item.season {
                let after = item.episode_offset.max(0);
                let count = match (item.progress, completed) {
                    (Some(n), _) => Some(n),
                    (None, true) => Some(item.episode_total.unwrap_or(i64::MAX - after)),
                    (None, false) => None,
                };
                if let Some(count) = count.filter(|n| *n > 0) {
                    tick_season(&state, title_id, &detail, season, after, after + count).await?;
                }
            }
        }

        // A diary entry is the whole point of importing a diary.
        if let Some(watched_at) = item.watched_at.as_deref() {
            let exists: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM watch_event
                  WHERE title_id = ? AND episode_id IS NULL AND watched_at = ?",
            )
            .bind(title_id)
            .bind(watched_at)
            .fetch_one(&state.pool)
            .await?;

            if exists.0 == 0 {
                sqlx::query(
                    "INSERT INTO watch_event (title_id, episode_id, watched_at, is_rewatch, source)
                     VALUES (?, NULL, ?, ?, 'import')",
                )
                .bind(title_id)
                .bind(watched_at)
                .bind(i64::from(item.rewatch))
                .execute(&state.pool)
                .await?;
            }
        }

        if already {
            updated += 1;
        } else {
            added += 1;
        }
    }

    Ok(Json(json!({
        "added": added,
        "updated": updated,
        "skipped": skipped,
        "failed": failed,
    })))
}

/// Mark episodes `after + 1 ..= upto` of a season watched.
///
/// Trackers record how far you got, not when, so these watches are undated:
/// they count towards progress and stats but stay out of the diary rather
/// than all landing on the day of the import.
async fn tick_season(
    state: &AppState,
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
        store::sync_all_seasons(state, title_id, detail).await?;
    }

    sqlx::query(
        "INSERT INTO watch_event (title_id, episode_id, watched_at, is_rewatch, source, undated)
         SELECT ?, e.id, ?, 0, 'import', 1
           FROM episode e
          WHERE e.title_id = ? AND e.season_number = ?
            AND e.episode_number > ? AND e.episode_number <= ?
            AND (e.air_date IS NULL OR e.air_date <= date('now'))
            AND NOT EXISTS (SELECT 1 FROM watch_event w WHERE w.episode_id = e.id)",
    )
    .bind(title_id)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(title_id)
    .bind(season)
    .bind(after)
    .bind(upto)
    .execute(&state.pool)
    .await?;

    Ok(())
}

/// (kind, tmdb_id, name, year, status, rating, favourite, notes, added, watches)
type ExportRow = (
    String,
    i64,
    String,
    Option<String>,
    String,
    Option<f64>,
    bool,
    Option<String>,
    String,
    i64,
);

/// (season, episode, my rating, my notes, watch count)
type EpisodeExportRow = (i64, i64, Option<f64>, Option<String>, i64);

/// (unused, watched_at, is_rewatch, source)
type WatchExportRow = (Option<String>, String, i64, String);

async fn export_rows(state: &AppState) -> AppResult<Vec<ExportRow>> {
    Ok(sqlx::query_as(
        "SELECT t.kind, t.tmdb_id, t.name, t.release_date, l.status, l.rating, l.favourite,
                l.notes, l.added_at,
                (SELECT COUNT(*) FROM watch_event w WHERE w.title_id = t.id)
           FROM library_entry l JOIN title t ON t.id = l.title_id
          ORDER BY t.name COLLATE NOCASE",
    )
    .fetch_all(&state.pool)
    .await?)
}

/// A complete backup: everything needed to rebuild the library elsewhere.
pub async fn export_json(_: Auth, State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let rows = export_rows(&state).await?;

    let mut titles = Vec::with_capacity(rows.len());
    for (kind, tmdb_id, name, release_date, status, rating, favourite, notes, added_at, _) in rows {
        let title_id = store::find_title_id(&state.pool, &kind, tmdb_id).await?;

        let episodes: Vec<EpisodeExportRow> = match title_id {
            Some(id) => {
                sqlx::query_as(
                    "SELECT e.season_number, e.episode_number, s.rating, s.notes,
                            (SELECT COUNT(*) FROM watch_event w WHERE w.episode_id = e.id)
                       FROM episode e
                       LEFT JOIN episode_state s ON s.episode_id = e.id
                      WHERE e.title_id = ?
                        AND (s.rating IS NOT NULL OR s.notes IS NOT NULL
                             OR EXISTS (SELECT 1 FROM watch_event w WHERE w.episode_id = e.id))
                      ORDER BY e.season_number, e.episode_number",
                )
                .bind(id)
                .fetch_all(&state.pool)
                .await?
            }
            None => Vec::new(),
        };

        let watches: Vec<WatchExportRow> = match title_id {
            Some(id) => {
                sqlx::query_as(
                    "SELECT NULL, w.watched_at, w.is_rewatch, w.source
                       FROM watch_event w
                      WHERE w.title_id = ? AND w.episode_id IS NULL
                      ORDER BY w.watched_at",
                )
                .bind(id)
                .fetch_all(&state.pool)
                .await?
            }
            None => Vec::new(),
        };

        titles.push(json!({
            "kind": kind,
            "tmdb_id": tmdb_id,
            "name": name,
            "release_date": release_date,
            "status": status,
            "rating": rating,
            "favourite": favourite,
            "notes": notes,
            "added_at": added_at,
            "episodes": episodes
                .into_iter()
                .map(|(s, e, rating, notes, watched)| json!({
                    "season": s, "episode": e, "rating": rating,
                    "notes": notes, "watched": watched > 0,
                }))
                .collect::<Vec<_>>(),
            "watches": watches
                .into_iter()
                .map(|(_, at, rewatch, source)| json!({
                    "watched_at": at, "rewatch": rewatch == 1, "source": source,
                }))
                .collect::<Vec<_>>(),
        }));
    }

    let settings: Option<(String,)> =
        sqlx::query_as("SELECT value FROM app_setting WHERE key = 'preferences'")
            .fetch_optional(&state.pool)
            .await?;

    let body = json!({
        "format": "frog-library",
        "version": 1,
        "exported_at": Utc::now().to_rfc3339(),
        "app_version": env!("CARGO_PKG_VERSION"),
        "settings": settings
            .and_then(|(v,)| serde_json::from_str::<Value>(&v).ok())
            .unwrap_or_else(|| json!({})),
        "titles": titles,
    });

    Ok(download(
        "frog-library.json",
        "application/json; charset=utf-8",
        serde_json::to_string_pretty(&body)?,
    ))
}

/// A flat sheet, for anything that reads CSV.
pub async fn export_csv(_: Auth, State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let rows = export_rows(&state).await?;

    let mut out = String::from("Type,TMDB ID,Title,Year,Status,Rating,Favourite,Added,Watches\n");
    for (kind, tmdb_id, name, release_date, status, rating, favourite, _, added_at, watches) in rows
    {
        let year = release_date.unwrap_or_default();
        let year = year.get(0..4).unwrap_or("");
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            if kind == "tv" { "Show" } else { "Film" },
            tmdb_id,
            csv_field(&name),
            year,
            status,
            rating.map(|r| format!("{r:.0}")).unwrap_or_default(),
            if favourite { "yes" } else { "no" },
            added_at.get(0..10).unwrap_or(""),
            watches
        ));
    }

    Ok(download("frog-library.csv", "text/csv; charset=utf-8", out))
}

/// Quote anything that would otherwise break a cell.
fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn download(filename: &str, content_type: &'static str, body: String) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    (headers, body)
}

#[cfg(test)]
mod tests {
    use super::csv_field;

    #[test]
    fn csv_quotes_only_what_needs_it() {
        assert_eq!(csv_field("Heat"), "Heat");
        assert_eq!(
            csv_field("Crouching Tiger, Hidden Dragon"),
            "\"Crouching Tiger, Hidden Dragon\""
        );
        assert_eq!(csv_field("He said \"hi\""), "\"He said \"\"hi\"\"\"");
    }
}
