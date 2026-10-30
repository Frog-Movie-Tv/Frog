//! What other sites make of a title, kept on Frog's own title.
//!
//! Frog is the catalog; sites contribute. A title's scores all sit in
//! `title_rating` under Frog's id, however the title came to be here, and a
//! score is only ever matched by provider id, never by name:
//!
//! - TMDB's id, when the title has one. An anime Frog keeps from AniList has
//!   none of its own, but the community list (`anime_map`) can say which TMDB
//!   entry it is. That id is only ever read through: it is not linked to the
//!   title, because the sync decides when an entry moves to TMDB.
//! - AniList's id, from `title_external` or `anime_map`. TMDB's one long show
//!   is several AniList entries; the first one scores the show.
//! - MyAnimeList's id, from AniList, for what TMDB has no entry for at all.
//!
//! MDBList carries IMDb, Rotten Tomatoes, Metacritic and the rest. Its free
//! tier allows 1,000 requests a day, so the worker looks at a few titles at a
//! time, in batches, and a title is not asked about again for days
//! (`next_check`). Without a key, MDBList is simply not asked.

use std::sync::atomic::Ordering;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::anilist::Api;
use crate::catalog;
use crate::config::MdbListClient;
use crate::error::AppResult;
use crate::state::AppState;
use crate::tmdb::models;

/// How the sites are listed, the ones people know first.
const ORDER: [&str; 11] = [
    "imdb",
    "tomatoes",
    "popcorn",
    "metacritic",
    "metacriticuser",
    "letterboxd",
    "trakt",
    "tmdb",
    "anilist",
    "myanimelist",
    "rogerebert",
];

/// MDBList's answer to one batch holds at most this many ids.
const BATCH: usize = 10;
/// Batches one pass of the worker may ask for.
const BATCHES_PER_PASS: usize = 2;
/// Requests left today below which the worker stops, so opening a title
/// still finds its scores.
const WORKER_FLOOR: i64 = 200;
/// How long a title page waits for a first look before showing what it has.
const FIRST_LOOK: std::time::Duration = std::time::Duration::from_millis(3500);

fn budget_left(client: &MdbListClient, floor: i64) -> bool {
    let budget = &client.budget;
    Utc::now().timestamp() >= budget.resets_at.load(Ordering::Relaxed)
        || budget.remaining.load(Ordering::Relaxed) > floor
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Rating {
    pub source: String,
    /// 0-100, whatever the site itself uses.
    pub score: f64,
    pub votes: Option<i64>,
}

/// One title's answer from MDBList, ready to store.
#[derive(Debug, Default, PartialEq)]
pub struct Found {
    pub tmdb_id: Option<i64>,
    pub ratings: Vec<Rating>,
}

/// MDBList's ratings of one item. A source it does not have comes back with
/// nulls, and a Roger Ebert score only as stars out of four.
pub fn parse_item(item: &Value) -> Found {
    let ratings = item["ratings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let source = r["source"].as_str()?;
            if !ORDER.contains(&source) {
                return None;
            }
            let score = r["score"].as_f64().or_else(|| {
                (source == "rogerebert")
                    .then(|| r["value"].as_f64().map(|stars| stars * 25.0))
                    .flatten()
            })?;
            (score > 0.0).then(|| Rating {
                source: source.to_string(),
                score,
                votes: r["votes"].as_i64(),
            })
        })
        .collect();
    Found {
        tmdb_id: item.pointer("/ids/tmdb").and_then(Value::as_i64),
        ratings,
    }
}

/// When to look at a title again: often while it is new and scores still
/// move, rarely once they have settled, and not soon when nothing was found.
pub fn next_check(release: Option<&str>, found: bool, now: DateTime<Utc>) -> DateTime<Utc> {
    if !found {
        return now + Duration::days(30);
    }
    let age = release
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .map(|d| (now.date_naive() - d).num_days());
    now + Duration::days(match age {
        Some(days) if days < 60 => 3,
        Some(days) if days < 365 => 14,
        _ => 45,
    })
}

/// Why MDBList gave nothing this time. Either way what is stored stays.
#[derive(Debug)]
enum Failure {
    /// The daily limit, or a refused key: nothing more until it resets.
    Limited,
    Other(String),
}

/// Reqwest's errors carry the address, and MDBList's address carries the key,
/// so nothing from a request is ever logged as it stands.
fn quiet(e: reqwest::Error) -> Failure {
    Failure::Other(e.without_url().to_string())
}

async fn call(
    state: &AppState,
    client: &MdbListClient,
    path: &str,
    body: Option<Value>,
) -> Result<Option<Value>, Failure> {
    let url = format!(
        "{}{path}?apikey={}",
        client.api_url,
        urlencoding::encode(&client.key)
    );
    let request = match body {
        Some(body) => state.http.post(&url).json(&body),
        None => state.http.get(&url),
    };
    let resp = request
        .header("accept", "application/json")
        .send()
        .await
        .map_err(quiet)?;

    let header = |name: &str| {
        resp.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<i64>().ok())
    };
    if let Some(left) = header("x-ratelimit-remaining") {
        client.budget.remaining.store(left, Ordering::Relaxed);
    }
    if let Some(reset) = header("x-ratelimit-reset") {
        client.budget.resets_at.store(reset, Ordering::Relaxed);
    }

    match resp.status().as_u16() {
        200 => resp.json().await.map(Some).map_err(quiet),
        404 => Ok(None),
        401 | 402 | 403 | 429 => {
            // Nothing is asked of it for an hour, unless it said how long.
            client.budget.remaining.store(0, Ordering::Relaxed);
            if header("x-ratelimit-reset").is_none() {
                let later = (Utc::now() + Duration::hours(1)).timestamp();
                client.budget.resets_at.store(later, Ordering::Relaxed);
            }
            Err(Failure::Limited)
        }
        status => Err(Failure::Other(format!("status {status}"))),
    }
}

/// What the title is known as outside Frog, as far as ratings go.
#[derive(Debug, Default)]
struct Ids {
    /// `movie` or `tv`, and TMDB's id: its own, or the one `anime_map` says.
    tmdb: Option<(String, i64)>,
    /// Whether that TMDB id came by way of AniList rather than being the
    /// title's own.
    borrowed: bool,
    anilist: Option<i64>,
    primary_anilist: bool,
}

async fn ids_of(pool: &SqlitePool, title_id: i64) -> AppResult<Ids> {
    let external = catalog::externals(pool, title_id).await?;
    let primary_anilist = external.primary == Some("anilist");
    let tmdb = external.tmdb.as_ref().map(|t| (t.kind.clone(), t.id));

    // TMDB's one show is several AniList entries, first season first.
    let mapped: Option<(i64,)> = match &tmdb {
        Some((kind, id)) => {
            sqlx::query_as(
                "SELECT anilist_id FROM anime_map
                  WHERE tmdb_kind = ? AND tmdb_id = ?
                  ORDER BY tmdb_season IS NULL, tmdb_season, tmdb_offset IS NULL, tmdb_offset,
                           anilist_id
                  LIMIT 1",
            )
            .bind(kind)
            .bind(id)
            .fetch_optional(pool)
            .await?
        }
        None => None,
    };
    let anilist = external.anilist.first().copied().or(mapped.map(|(id,)| id));

    let (tmdb, borrowed) = match (tmdb, anilist) {
        (Some(own), _) => (Some(own), false),
        (None, Some(media_id)) => {
            let found: Option<(String, i64)> = sqlx::query_as(
                "SELECT tmdb_kind, tmdb_id FROM anime_map
                  WHERE anilist_id = ? AND tmdb_kind IN ('movie', 'tv') AND tmdb_id IS NOT NULL",
            )
            .bind(media_id)
            .fetch_optional(pool)
            .await?;
            let borrowed = found.is_some();
            (found, borrowed)
        }
        (None, None) => (None, false),
    };
    Ok(Ids {
        tmdb,
        borrowed,
        anilist,
        primary_anilist,
    })
}

const SCORES_QUERY: &str = "query ($ids: [Int]) {
  Page(perPage: 50) { media(id_in: $ids, type: ANIME) { id idMal averageScore popularity } }
}";

/// AniList's own score of some of its entries, and their MyAnimeList ids.
/// Public data, read without anyone's token.
async fn anilist_scores(state: &AppState, ids: &[i64]) -> Vec<(i64, Option<f64>, Option<i64>)> {
    let Ok(api) = Api::new(state) else {
        return Vec::new();
    };
    match api.graphql("", SCORES_QUERY, json!({ "ids": ids })).await {
        Ok(data) => data
            .pointer("/Page/media")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|m| {
                Some((
                    m["id"].as_i64()?,
                    m["averageScore"].as_f64().filter(|s| *s > 0.0),
                    m["idMal"].as_i64(),
                ))
            })
            .collect(),
        Err(e) => {
            tracing::debug!(error = %e, "AniList scores not read");
            Vec::new()
        }
    }
}

async fn store(
    pool: &SqlitePool,
    title_id: i64,
    ratings: &[Rating],
    now: DateTime<Utc>,
) -> AppResult<()> {
    for r in ratings {
        sqlx::query(
            "INSERT INTO title_rating (title_id, source, score, votes, fetched_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (title_id, source) DO UPDATE SET
                score = excluded.score, votes = excluded.votes,
                fetched_at = excluded.fetched_at",
        )
        .bind(title_id)
        .bind(&r.source)
        .bind(r.score)
        .bind(r.votes)
        .bind(now.to_rfc3339())
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn mark_checked(
    pool: &SqlitePool,
    title_id: i64,
    age_rating: Option<&str>,
    next: DateTime<Utc>,
    now: DateTime<Utc>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO title_rating_check (title_id, age_rating, checked_at, next_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT (title_id) DO UPDATE SET
            age_rating = COALESCE(excluded.age_rating, title_rating_check.age_rating),
            checked_at = excluded.checked_at, next_at = excluded.next_at",
    )
    .bind(title_id)
    .bind(age_rating)
    .bind(now.to_rfc3339())
    .bind(next.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// What TMDB can add to a title it is not the source of, through the id the
/// community list gives: its score, its age rating and its logo. A title
/// keeps a logo it already has; `logo_path` is the only thing written to it.
async fn from_borrowed_tmdb(
    state: &AppState,
    title_id: i64,
    kind: &str,
    tmdb_id: i64,
    now: DateTime<Utc>,
) -> Option<String> {
    let detail = match kind {
        "movie" => state.tmdb.movie(tmdb_id).await,
        _ => state.tmdb.tv(tmdb_id).await,
    };
    let detail = match detail {
        Ok(detail) => detail,
        Err(e) => {
            tracing::debug!(title_id, tmdb_id, error = %e, "TMDB's side not read");
            return None;
        }
    };
    if let Some(score) = models::f64_at(&detail, "vote_average").filter(|v| *v > 0.0) {
        let rating = Rating {
            source: "tmdb".into(),
            score: score * 10.0,
            votes: models::i64_at(&detail, "vote_count"),
        };
        if let Err(e) = store(&state.pool, title_id, &[rating], now).await {
            tracing::warn!(title_id, error = %e, "TMDB score not stored");
        }
    }
    if let Some(core) = models::TitleCore::from_detail(kind, &detail) {
        if let Some(logo) = core.logo_path {
            if let Err(e) =
                sqlx::query("UPDATE title SET logo_path = COALESCE(logo_path, ?) WHERE id = ?")
                    .bind(logo)
                    .bind(title_id)
                    .execute(&state.pool)
                    .await
            {
                tracing::warn!(title_id, error = %e, "logo not stored");
            }
        }
    }
    models::age_rating(&detail, state.tmdb.region())
}

/// Look up a set of titles, as few requests as it takes: one batch for each
/// kind on MDBList, one for AniList. A failed request leaves those titles to
/// be tried again; a title nobody knows waits a while.
pub async fn refresh(state: &AppState, titles: &[i64], floor: i64) -> AppResult<()> {
    let pool = &state.pool;
    let now = Utc::now();

    let mut resolved = Vec::with_capacity(titles.len());
    for title_id in titles {
        resolved.push((*title_id, ids_of(pool, *title_id).await?));
    }

    // AniList first: its answer also holds the MyAnimeList ids.
    let wanted: Vec<i64> = resolved.iter().filter_map(|(_, i)| i.anilist).collect();
    let scores = if wanted.is_empty() {
        Vec::new()
    } else {
        anilist_scores(state, &wanted).await
    };
    let mut mal = Vec::new();
    for (title_id, ids) in &resolved {
        let Some(media) = ids.anilist else { continue };
        let Some((_, score, id_mal)) = scores.iter().find(|s| s.0 == media) else {
            continue;
        };
        if let (Some(score), false) = (score, ids.primary_anilist) {
            let rating = Rating {
                source: "anilist".into(),
                score: *score,
                votes: None,
            };
            store(pool, *title_id, &[rating], now).await?;
        }
        if ids.tmdb.is_none() {
            if let Some(id) = id_mal {
                mal.push((*title_id, *id));
            }
        }
    }

    // Everything else TMDB can say about a title it does not supply.
    let mut ages: Vec<(i64, Option<String>)> = Vec::new();
    for (title_id, ids) in &resolved {
        let age = match (&ids.tmdb, ids.borrowed) {
            (Some((kind, id)), true) => from_borrowed_tmdb(state, *title_id, kind, *id, now).await,
            (Some((kind, id)), false) => state
                .tmdb
                .kept(kind, *id)
                .await
                .and_then(|(detail, _)| models::age_rating(&detail, state.tmdb.region())),
            _ => None,
        };
        ages.push((*title_id, age));
    }

    let mut found: Vec<(i64, Found)> = Vec::new();
    let mut failed: Vec<i64> = Vec::new();
    if let Some(client) = state.config.mdblist.as_ref() {
        for (kind, media_type) in [("movie", "movie"), ("tv", "show")] {
            let group: Vec<(i64, i64)> = resolved
                .iter()
                .filter_map(|(title_id, ids)| {
                    let (k, id) = ids.tmdb.as_ref()?;
                    (k == kind).then_some((*title_id, *id))
                })
                .collect();
            for chunk in group.chunks(BATCH) {
                if !budget_left(client, floor) {
                    failed.extend(chunk.iter().map(|c| c.0));
                    continue;
                }
                let ids: Vec<i64> = chunk.iter().map(|c| c.1).collect();
                let path = format!("/tmdb/{media_type}/");
                match call(state, client, &path, Some(json!({ "ids": ids }))).await {
                    Ok(answer) => {
                        let items = answer.unwrap_or(Value::Null);
                        for item in items.as_array().into_iter().flatten() {
                            let parsed = parse_item(item);
                            let title = chunk.iter().find(|c| Some(c.1) == parsed.tmdb_id);
                            if let Some((title_id, _)) = title {
                                found.push((*title_id, parsed));
                            }
                        }
                    }
                    Err(e) => {
                        log_failure(&e);
                        failed.extend(chunk.iter().map(|c| c.0));
                    }
                }
            }
        }
        // No batch for MyAnimeList's ids, and they are few.
        for (title_id, id) in mal {
            if !budget_left(client, floor) {
                failed.push(title_id);
                continue;
            }
            match call(state, client, &format!("/mal/any/{id}/"), None).await {
                Ok(Some(item)) => found.push((title_id, parse_item(&item))),
                Ok(None) => {}
                Err(e) => {
                    log_failure(&e);
                    failed.push(title_id);
                }
            }
        }
    }

    for (title_id, _) in &resolved {
        if failed.contains(title_id) {
            continue;
        }
        let answer = found.iter().find(|f| f.0 == *title_id);
        if let Some((_, parsed)) = answer {
            store(pool, *title_id, &parsed.ratings, now).await?;
        }
        let release: Option<(Option<String>,)> =
            sqlx::query_as("SELECT release_date FROM title WHERE id = ?")
                .bind(title_id)
                .fetch_optional(pool)
                .await?;
        let date = release.and_then(|r| r.0);
        let age = ages
            .iter()
            .find(|a| a.0 == *title_id)
            .and_then(|a| a.1.as_deref());
        let next = next_check(date.as_deref(), answer.is_some(), now);
        mark_checked(pool, *title_id, age, next, now).await?;
    }
    Ok(())
}

fn log_failure(e: &Failure) {
    match e {
        Failure::Limited => tracing::info!("MDBList's daily limit reached; waiting"),
        Failure::Other(detail) => tracing::warn!(%detail, "MDBList request failed"),
    }
}

/// Titles somebody tracks whose scores are due, a few at a time.
pub async fn refresh_due(state: &AppState) {
    let due: Vec<(i64,)> = sqlx::query_as(
        "SELECT DISTINCT t.id
           FROM library_entry l
           JOIN title t ON t.id = l.title_id
           LEFT JOIN title_rating_check c ON c.title_id = t.id
          WHERE t.merged_into IS NULL AND (c.next_at IS NULL OR c.next_at <= ?)
          ORDER BY c.next_at IS NOT NULL, c.next_at
          LIMIT ?",
    )
    .bind(Utc::now().to_rfc3339())
    .bind((BATCH * BATCHES_PER_PASS) as i64)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    if due.is_empty() {
        return;
    }
    let ids: Vec<i64> = due.into_iter().map(|d| d.0).collect();
    if let Err(e) = refresh(state, &ids, WORKER_FLOOR).await {
        tracing::warn!(error = %e, "title ratings not refreshed");
    }
}

/// (source, score, votes)
type RatingRow = (String, f64, Option<i64>);

/// Everything Frog has on a title's scores, in a steady order. `primary_vote`
/// is what the title's own provider says, out of ten.
pub async fn of_title(pool: &SqlitePool, title_id: i64) -> AppResult<Value> {
    let rows: Vec<RatingRow> =
        sqlx::query_as("SELECT source, score, votes FROM title_rating WHERE title_id = ?")
            .bind(title_id)
            .fetch_all(pool)
            .await?;
    let (vote,): (Option<f64>,) = sqlx::query_as("SELECT tmdb_vote FROM title WHERE id = ?")
        .bind(title_id)
        .fetch_one(pool)
        .await?;
    let external = catalog::externals(pool, title_id).await?;
    let own = external.primary.filter(|_| vote.is_some());

    let mut ratings: Vec<Rating> = rows
        .into_iter()
        .filter(|(source, _, _)| Some(source.as_str()) != own)
        .map(|(source, score, votes)| Rating {
            source,
            score,
            votes,
        })
        .collect();
    if let (Some(source), Some(vote)) = (own, vote) {
        ratings.push(Rating {
            source: source.to_string(),
            score: (vote * 10.0).round(),
            votes: None,
        });
    }
    let place = |r: &Rating| {
        ORDER
            .iter()
            .position(|s| *s == r.source)
            .unwrap_or(ORDER.len())
    };
    ratings.sort_by_key(place);

    let age: Option<(Option<String>,)> =
        sqlx::query_as("SELECT age_rating FROM title_rating_check WHERE title_id = ?")
            .bind(title_id)
            .fetch_optional(pool)
            .await?;
    Ok(json!({ "ratings": ratings, "age_rating": age.and_then(|a| a.0) }))
}

/// A title's scores for its page. The first time anyone opens it, a quick
/// look, held to a moment so the page never waits on another site; whatever
/// did not make it is there next time.
pub async fn for_page(state: &AppState, title_id: i64) -> AppResult<Value> {
    let checked: Option<(String,)> =
        sqlx::query_as("SELECT next_at FROM title_rating_check WHERE title_id = ?")
            .bind(title_id)
            .fetch_optional(&state.pool)
            .await?;
    if checked.is_none() {
        let state_for_look = state.clone();
        let look = tokio::spawn(async move { refresh(&state_for_look, &[title_id], 20).await });
        if let Ok(Ok(Err(e))) = tokio::time::timeout(FIRST_LOOK, look).await {
            tracing::warn!(title_id, error = %e, "title ratings not read");
        }
    }
    of_title(&state.pool, title_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_without_a_score_is_left_out() {
        // MDBList's answer for Fight Club, cut down.
        let item = json!({
            "ids": { "imdb": "tt0137523", "tmdb": 550, "mal": null },
            "ratings": [
                { "source": "imdb", "value": 8.8, "score": 88, "votes": 2678118, "url": 167 },
                { "source": "tomatoes", "value": 81, "score": 81, "votes": 251, "fresh": 1 },
                { "source": "popcorn", "value": 96, "score": 96, "votes": 73867 },
                { "source": "rogerebert", "value": 2.0, "score": null, "votes": null },
                { "source": "myanimelist", "value": null, "score": null, "votes": null },
                { "source": "somewhere-new", "value": 5, "score": 50, "votes": 1 }
            ]
        });
        let found = parse_item(&item);
        assert_eq!(found.tmdb_id, Some(550));
        let got: Vec<(&str, f64)> = found
            .ratings
            .iter()
            .map(|r| (r.source.as_str(), r.score))
            .collect();
        assert_eq!(
            got,
            [
                ("imdb", 88.0),
                ("tomatoes", 81.0),
                ("popcorn", 96.0),
                ("rogerebert", 50.0)
            ]
        );
    }

    #[test]
    fn new_titles_are_looked_at_often_and_old_ones_rarely() {
        let now = "2026-10-30T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let days = |d: Option<&str>, found| (next_check(d, found, now) - now).num_days();
        assert_eq!(days(Some("2026-10-01"), true), 3);
        assert_eq!(days(Some("2026-03-01"), true), 14);
        assert_eq!(days(Some("1999-10-15"), true), 45);
        assert_eq!(days(None, true), 45);
        assert_eq!(days(Some("2026-10-01"), false), 30);
    }
}
