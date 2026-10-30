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
use futures_util::{stream, StreamExt};
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
    /// With no `kind`: the kind to take when a name is both a film and a show.
    pub prefer: Option<String>,
    /// The source's own name for the season ("Stone Ocean"), which picks
    /// between shows that share a name: only one of them has that season.
    pub season_name: Option<String>,
    /// Episode names the source gives, to be found on TMDB by name when the
    /// season is not known for sure.
    #[serde(default)]
    pub episodes: Vec<String>,
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
    /// A matched show's seasons, so the client can tell whether every episode
    /// was seen and place seasons it only knows by name.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub seasons: Vec<SeasonCount>,
    /// The name that found the match, when it was not the row's own title.
    pub searched: Option<String>,
    /// Which of the row's `episodes` TMDB has, and where.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub placed: Vec<Placed>,
}

#[derive(Debug, Serialize)]
pub struct Placed {
    /// Index into the row's `episodes`.
    pub index: usize,
    pub season: i64,
    pub episode: i64,
}

#[derive(Debug, Serialize)]
pub struct SeasonCount {
    pub number: i64,
    pub name: String,
    pub episodes: i64,
}

#[derive(Debug, Deserialize)]
pub struct MatchRequest {
    pub items: Vec<Incoming>,
}

/// Matching is capped so one request cannot sit on TMDB for ten minutes; the
/// wizard sends a few hundred rows at a time and shows progress.
const MAX_BATCH: usize = 200;

/// Rows looked up side by side within a batch: enough that a big import does
/// not wait on TMDB one row at a time, few enough to stay well under its limit.
const MATCH_AT_ONCE: usize = 6;

pub async fn match_titles(
    auth: Auth,
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
           FROM library_entry l JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ?",
    )
    .bind(auth.user_id())
    .fetch_all(&state.pool)
    .await?;

    // `buffered` keeps the rows in the order they came.
    let out: Vec<MatchResult> = stream::iter(body.items)
        .map(|row| match_row(&state, &tracked, row))
        .buffered(MATCH_AT_ONCE)
        .collect()
        .await;

    Ok(Json(json!({ "results": out })))
}

/// Everything the wizard needs to know about one row.
async fn match_row(
    state: &AppState,
    tracked: &[(String, i64, String)],
    row: Incoming,
) -> MatchResult {
    let Resolved {
        matched,
        candidates,
        mut note,
        searched,
    } = resolve(state, &row).await;
    let mut last_season = None;
    let mut season_counts = Vec::new();
    let mut placed = Vec::new();

    if let Some(m) = matched.as_ref().filter(|m| m.kind == "tv") {
        if let Ok(detail) = state.tmdb.tv(m.tmdb_id).await {
            let seasons = crate::tmdb::models::seasons(&detail);
            last_season = seasons.iter().map(|s| s.season_number).max();
            // A season the show does not have is the anime case that needs
            // a human: some shows are split into seasons differently on TMDB.
            if let Some(season) = row.season {
                if !seasons.iter().any(|s| s.season_number == season) {
                    note = Some(format!(
                        "TMDB lists no season {season} for this show; check the match"
                    ));
                }
            }
            if !row.episodes.is_empty() {
                placed = find_episodes(state, m.tmdb_id, &row, &seasons, note.is_some()).await;
            }
            season_counts = seasons
                .into_iter()
                .map(|s| SeasonCount {
                    number: s.season_number,
                    name: s.name,
                    episodes: s.episode_count,
                })
                .collect();
        }
    }

    let already = matched.as_ref().and_then(|m| {
        tracked
            .iter()
            .find(|(kind, id, _)| *kind == m.kind && *id == m.tmdb_id)
            .map(|(_, _, status)| status.clone())
    });

    MatchResult {
        key: row.key,
        matched,
        candidates,
        note,
        already_tracked: already,
        season: row.season,
        last_season,
        seasons: season_counts,
        searched,
        placed,
    }
}

/// Look a row's episode names up on TMDB.
///
/// A row sure of its season reads only that one, so a big import stays quick.
/// A row in doubt reads the whole show, specials left out.
async fn find_episodes(
    state: &AppState,
    tmdb_id: i64,
    row: &Incoming,
    seasons: &[crate::tmdb::models::SeasonSummary],
    in_doubt: bool,
) -> Vec<Placed> {
    if let Some(season) = row.season.filter(|_| !in_doubt) {
        return place_episodes(state, tmdb_id, &[season], &row.episodes).await;
    }
    let every: Vec<i64> = seasons
        .iter()
        .map(|s| s.season_number)
        .filter(|n| *n > 0)
        .collect();
    place_episodes(state, tmdb_id, &every, &row.episodes).await
}

/// What `resolve` decided for one row.
#[derive(Default)]
struct Resolved {
    matched: Option<SearchItem>,
    candidates: Vec<SearchItem>,
    note: Option<String>,
    /// Set when one of the other names found it, not the title itself.
    searched: Option<String>,
}

impl Resolved {
    fn exact(item: SearchItem) -> Self {
        Resolved {
            matched: Some(item),
            ..Default::default()
        }
    }

    fn missing(note: &str) -> Self {
        Resolved {
            note: Some(note.into()),
            ..Default::default()
        }
    }
}

/// Work out which TMDB title a row means.
///
/// An IMDb id is unambiguous, so it wins. Otherwise search by name and prefer
/// an exact title in the right year: a year match is what separates the 1978
/// film from the 2008 remake.
async fn resolve(state: &AppState, row: &Incoming) -> Resolved {
    if let (Some(tmdb_id), Some(kind)) = (row.tmdb_id, row.kind.as_deref()) {
        if kind == "movie" || kind == "tv" {
            if let Ok(detail) = fetch_known(state, kind, tmdb_id).await {
                return Resolved::exact(detail);
            }
        }
    }

    if let Some(imdb) = row.imdb_id.as_deref().filter(|s| s.starts_with("tt")) {
        if let Some(found) = find_by_imdb(state, imdb).await {
            return Resolved::exact(found);
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
            Err(_) => return Resolved::missing("TMDB did not answer"),
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
        return Resolved::missing("No match found");
    }
    let other_name = (searched != row.title).then(|| searched.clone());

    // A live-action remake shares a name with half of anime; for rows from
    // an anime tracker, animated results go first.
    if row.anime {
        const ANIMATION: i64 = 16;
        found.sort_by_key(|f| !f.genre_ids.contains(&ANIMATION));
    }
    if let Some(prefer) = row.prefer.as_deref() {
        found.sort_by_key(|f| f.kind != prefer);
    }

    let wanted = searched.trim().to_lowercase();

    // Two shows called "One Piece", two called "Avatar: The Last Airbender":
    // the one meant has the season the row is about.
    if let Some(i) = pick_by_season(state, &found, &searched, row).await {
        let best = found.remove(i);
        return Resolved {
            matched: Some(best),
            candidates: found.into_iter().take(4).collect(),
            note: None,
            searched: other_name,
        };
    }

    let exact_same_year = found
        .iter()
        .position(|f| f.name.to_lowercase() == wanted && row.year.is_some() && f.year == row.year);

    if let Some(i) = exact_same_year {
        let best = found.remove(i);
        return Resolved {
            matched: Some(best),
            candidates: found.into_iter().take(4).collect(),
            note: None,
            searched: other_name,
        };
    }

    // Netflix writes the 2023 series "ONE PIECE" and the anime "One Piece":
    // the same letters in the same case win over the same letters.
    let exact = found
        .iter()
        .position(|f| f.name == searched.trim())
        .or_else(|| found.iter().position(|f| f.name.to_lowercase() == wanted));
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
        // "Bleach" is a film and a show; the preferred kind won, but only
        // just.
        _ if row.prefer.is_some()
            && found
                .iter()
                .any(|f| f.kind != best.kind && f.name.to_lowercase() == wanted) =>
        {
            Some(if best.kind == "movie" {
                "There is also a show by this name".into()
            } else {
                "There is also a film by this name".into()
            })
        }
        _ => None,
    };

    Resolved {
        matched: Some(best),
        candidates: found.into_iter().take(4).collect(),
        note,
        searched: other_name,
    }
}

/// An episode name reduced to its letters and digits: "Chapter One: The
/// Vanishing of Will Byers" and "Chapter One - The Vanishing Of Will Byers"
/// are the same episode.
fn episode_key(name: &str) -> String {
    name.to_lowercase()
        .replace('&', "and")
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Find episodes by name across the given seasons of a show.
///
/// A name is placed only when exactly one episode has it; "Pilot" in two
/// seasons places nothing. Netflix sometimes keeps part of an episode name
/// apart ("Chapter One: The Vanishing of Will Byers"), so the last part on its
/// own is tried when the whole finds nothing.
async fn place_episodes(
    state: &AppState,
    tmdb_id: i64,
    seasons: &[i64],
    names: &[String],
) -> Vec<Placed> {
    let mut by_name: std::collections::HashMap<String, Vec<(i64, i64)>> =
        std::collections::HashMap::new();
    for season in seasons {
        let Ok(raw) = state.tmdb.season(tmdb_id, *season).await else {
            continue;
        };
        for e in crate::tmdb::models::episodes_of_season(&raw) {
            let key = episode_key(e.name.as_deref().unwrap_or(""));
            // TMDB fills unnamed episodes with "Episode 12"; those say nothing.
            let numbered =
                key.starts_with("episode") && key[7..].chars().all(|c| c.is_ascii_digit());
            if key.is_empty() || numbered {
                continue;
            }
            by_name
                .entry(key)
                .or_default()
                .push((e.season_number, e.episode_number));
        }
    }

    let mut out = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let tail = name.rsplit(": ").next().unwrap_or(name);
        let found = [episode_key(name), episode_key(tail)]
            .into_iter()
            .filter(|k| !k.is_empty())
            .find_map(|k| by_name.get(&k).filter(|hits| hits.len() == 1));
        if let Some(&[(season, episode)]) = found.map(Vec::as_slice) {
            out.push(Placed {
                index,
                season,
                episode,
            });
        }
    }
    out
}

/// Season names compared loosely: "Sun & Moon - Ultra Adventures" is
/// "Sun & Moon: Ultra Adventures", and "Egghead Arc" is "Egghead". "Part"
/// and "Season" stay: a "Part 2" is not a "Season 2".
pub fn season_words(name: &str) -> String {
    let lower = name.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .filter(|w| !matches!(*w, "the" | "arc" | "saga"))
        .collect();
    words.join(" ")
}

/// Among shows found under the same name, the first that has the row's
/// season: one of that name, or that number with at least as many episodes
/// as were watched. None when the name is not shared or nothing fits.
async fn pick_by_season(
    state: &AppState,
    found: &[SearchItem],
    searched: &str,
    row: &Incoming,
) -> Option<usize> {
    let wanted = searched.trim().to_lowercase();
    let target = row
        .season_name
        .as_deref()
        .map(season_words)
        .filter(|t| !t.is_empty());
    let number = row.season.filter(|n| *n > 0);
    if target.is_none() && number.is_none() {
        return None;
    }
    let mut same: Vec<usize> = found
        .iter()
        .enumerate()
        .filter(|(_, f)| f.kind == "tv" && f.name.to_lowercase() == wanted)
        .map(|(i, _)| i)
        .take(4)
        .collect();
    // The exact casing first, as for a plain match.
    same.sort_by_key(|&i| found[i].name != searched.trim());
    if same.len() < 2 {
        return None;
    }
    for i in same {
        let Ok(detail) = state.tmdb.tv(found[i].tmdb_id).await else {
            continue;
        };
        let hit = crate::tmdb::models::seasons(&detail).iter().any(|s| {
            if s.season_number < 1 {
                return false;
            }
            match (&target, number) {
                (Some(target), _) => {
                    let have = season_words(&s.name);
                    !have.is_empty()
                        && (have == *target
                            || format!(" {have} ").contains(&format!(" {target} "))
                            || format!(" {target} ").contains(&format!(" {have} ")))
                }
                (None, Some(n)) => {
                    s.season_number == n && s.episode_count >= row.progress.unwrap_or(1)
                }
                (None, None) => false,
            }
        });
        if hit {
            return Some(i);
        }
    }
    None
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
pub fn merged_status(existing: &str, incoming: &str) -> String {
    match (status_rank(existing), status_rank(incoming)) {
        (Some(have), Some(new)) if new > have => incoming.to_string(),
        _ => existing.to_string(),
    }
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

pub async fn apply(
    auth: Auth,
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
            Some(id) => store::get_entry(&state.pool, auth.user_id(), id)
                .await?
                .is_some(),
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
            let watched = store::progress(&state.pool, auth.user_id(), title_id)
                .await?
                .watched;
            if watched > 0 {
                "watching".to_string()
            } else {
                status
            }
        } else {
            status
        };

        // What a merge compares against, to report only real changes.
        let before = if already && body.on_conflict == Conflict::Merge {
            let entry = store::get_entry(&state.pool, auth.user_id(), title_id).await?;
            let watched = store::progress(&state.pool, auth.user_id(), title_id)
                .await?
                .watched;
            entry.map(|e| (e, watched))
        } else {
            None
        };

        let completed = status == "completed";
        let patch = match &before {
            Some((have, _)) => LibraryPatch {
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
        };
        store::apply_patch(&state.pool, auth.user_id(), title_id, &patch).await?;

        if item.kind == "tv" && !item.episodes.is_empty() {
            tick_episodes(&state, auth.user_id(), title_id, &detail, &item.episodes).await?;
        } else if item.kind == "tv" {
            if let Some(season) = item.season {
                let after = item.episode_offset.max(0);
                let count = match (item.progress, completed) {
                    (Some(n), _) => Some(n),
                    (None, true) => Some(item.episode_total.unwrap_or(i64::MAX - after)),
                    (None, false) => None,
                };
                if let Some(count) = count.filter(|n| *n > 0) {
                    tick_season(
                        &state,
                        auth.user_id(),
                        title_id,
                        &detail,
                        season,
                        after,
                        after + count,
                    )
                    .await?;
                }
            } else if completed && !already {
                // Finished, with no season given: the whole show was watched,
                // so its episodes count (undated) like any other.
                for s in crate::tmdb::models::seasons(&detail)
                    .into_iter()
                    .filter(|s| s.season_number > 0)
                {
                    tick_season(
                        &state,
                        auth.user_id(),
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

        // A finished film with no date still counts as watched: undated, so
        // it is in the totals but not on the day of the import in the diary.
        if item.kind == "movie" && completed && item.watched_at.is_none() {
            sqlx::query(
                "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch,
                                          source, undated)
                 SELECT ?1, ?2, NULL, ?3, 0, 'import', 1
                  WHERE NOT EXISTS (SELECT 1 FROM watch_event
                                     WHERE user_id = ?1 AND title_id = ?2 AND episode_id IS NULL)",
            )
            .bind(auth.user_id())
            .bind(title_id)
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&state.pool)
            .await?;
        }

        // A diary entry is the whole point of importing a diary. Only for a
        // film: a show's watches are its episodes, and a title-level entry
        // would count the show as a film in the stats.
        if let Some(watched_at) = item.watched_at.as_deref().filter(|_| item.kind == "movie") {
            let exists: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM watch_event
                  WHERE user_id = ? AND title_id = ? AND episode_id IS NULL
                    AND watched_at = ?",
            )
            .bind(auth.user_id())
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
                .bind(auth.user_id())
                .bind(title_id)
                .bind(watched_at)
                .bind(i64::from(item.rewatch))
                .execute(&state.pool)
                .await?;
            }
        }

        if let Some((have, watched_before)) = before {
            let now = store::get_entry(&state.pool, auth.user_id(), title_id).await?;
            let watched_now = store::progress(&state.pool, auth.user_id(), title_id)
                .await?
                .watched;
            let same = now.is_some_and(|e| {
                e.status == have.status && e.rating == have.rating && e.favourite == have.favourite
            }) && watched_now == watched_before;
            if same {
                skipped += 1;
            } else {
                updated += 1;
            }
        } else if already {
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
        store::sync_all_seasons(state, title_id, detail).await?;
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
            store::sync_all_seasons(state, title_id, detail).await?;
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

async fn export_rows(state: &AppState, user_id: i64) -> AppResult<Vec<ExportRow>> {
    Ok(sqlx::query_as(
        "SELECT t.kind, t.tmdb_id, t.name, t.release_date, l.status, l.rating, l.favourite,
                l.notes, l.added_at,
                (SELECT COUNT(*) FROM watch_event w
                  WHERE w.user_id = ?1 AND w.title_id = t.id)
           FROM library_entry l JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ?1
          ORDER BY t.name COLLATE NOCASE",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?)
}

/// A complete backup: everything needed to rebuild the library elsewhere.
pub async fn export_json(
    auth: Auth,
    State(state): State<AppState>,
) -> AppResult<impl IntoResponse> {
    let rows = export_rows(&state, auth.user_id()).await?;

    let mut titles = Vec::with_capacity(rows.len());
    for (kind, tmdb_id, name, release_date, status, rating, favourite, notes, added_at, _) in rows {
        let title_id = store::find_title_id(&state.pool, &kind, tmdb_id).await?;

        let episodes: Vec<EpisodeExportRow> = match title_id {
            Some(id) => {
                sqlx::query_as(
                    "SELECT e.season_number, e.episode_number, s.rating, s.notes,
                            (SELECT COUNT(*) FROM watch_event w
                              WHERE w.user_id = ?1 AND w.episode_id = e.id)
                       FROM episode e
                       LEFT JOIN episode_state s ON s.episode_id = e.id AND s.user_id = ?1
                      WHERE e.title_id = ?2
                        AND (s.rating IS NOT NULL OR s.notes IS NOT NULL
                             OR EXISTS (SELECT 1 FROM watch_event w
                                         WHERE w.user_id = ?1 AND w.episode_id = e.id))
                      ORDER BY e.season_number, e.episode_number",
                )
                .bind(auth.user_id())
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
                      WHERE w.user_id = ? AND w.title_id = ? AND w.episode_id IS NULL
                      ORDER BY w.watched_at",
                )
                .bind(auth.user_id())
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
        sqlx::query_as("SELECT value FROM app_setting WHERE user_id = ? AND key = 'preferences'")
            .bind(auth.user_id())
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
pub async fn export_csv(auth: Auth, State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let rows = export_rows(&state, auth.user_id()).await?;

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

#[cfg(test)]
mod merge_tests {
    use super::merged_status;

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
}
