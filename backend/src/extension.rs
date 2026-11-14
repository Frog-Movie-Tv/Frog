//! The browser extension: which title a page is playing, logging it, and
//! taking a log back.
//!
//! The extension knows nothing about sites. It sends whatever it found on a
//! page (provider ids, numbers, names) and `resolve` turns that into one of
//! Frog's titles with a confidence: `exact` from an id, `likely` or `guess`
//! from a name. Only an exact match with an episode it can name is logged
//! without asking.
//!
//! A log goes through the same steps as the title page's checkmark
//! (`store::add_watch`, then `release::after_watch`), so the status, finishing
//! a show and the diary behave the same. `extension_log` remembers each one,
//! which is what makes logging idempotent and undo exact.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;

use crate::anilist::locate;
use crate::catalog::{self, ExternalId, TmdbRef};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::store;
use crate::text::{self, Closeness};

/// The same episode from the same token within this long is the same watch:
/// a reload, a second tab, a replay of the last minutes.
pub const SAME_WATCH_HOURS: i64 = 6;

/// How many titles besides the best one `resolve` offers.
pub const ALTERNATIVES: usize = 5;

/// Anything longer than this from a page is cut before it is looked at.
const TEXT_MAX: usize = 300;
/// The largest id believed, Frog's or TMDB's: TMDB's are 32-bit.
const ID_MAX: i64 = i32::MAX as i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Guess,
    Likely,
    Exact,
}

/// What the extension found on a page. Every field is optional; the more
/// there is, the surer the answer.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct Signals {
    /// Frog's own id, from a rule the person taught the extension.
    pub title_id: Option<i64>,
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    /// "movie" or "tv".
    pub kind: Option<String>,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    /// An episode counted from the start of the show, as anime sites do.
    pub absolute: Option<i64>,
    pub title: Option<String>,
    pub year: Option<i32>,
    pub page_title: Option<String>,
}

impl Signals {
    /// Drop what cannot be right, so nothing below has to doubt it.
    pub fn checked(mut self) -> Self {
        let number = |n: Option<i64>, least: i64| n.filter(|n| (least..=100_000).contains(n));
        // Ids are not counts: TMDB's shows are past 250,000 and its films past
        // a million, and Frog's own ids only grow.
        let id = |n: Option<i64>| n.filter(|n| (1..=ID_MAX).contains(n));
        self.title_id = id(self.title_id);
        self.tmdb_id = id(self.tmdb_id);
        self.season = number(self.season, 0);
        self.episode = number(self.episode, 1);
        self.absolute = number(self.absolute, 1);
        self.year = self.year.filter(|y| (1870..=2100).contains(y));
        self.kind = self.kind.filter(|k| k == "movie" || k == "tv");
        self.imdb_id = self.imdb_id.and_then(|id| imdb(&id));
        let cut = |s: Option<String>| {
            s.map(|s| s.chars().take(TEXT_MAX).collect::<String>())
                .filter(|s| !s.trim().is_empty())
        };
        self.title = cut(self.title);
        self.page_title = cut(self.page_title);
        self
    }

    /// Numbers the extension did not send but its text holds ("S01E03").
    fn with_numbers_from_text(mut self) -> Self {
        if self.season.is_none() && self.episode.is_none() && self.absolute.is_none() {
            for text in [self.title.as_deref(), self.page_title.as_deref()]
                .into_iter()
                .flatten()
            {
                if let Some((season, episode)) = numbers_in(text) {
                    self.season = season;
                    self.episode = Some(episode);
                    break;
                }
            }
        }
        self
    }

    /// Whether the page talks about episodes at all.
    fn episodic(&self) -> bool {
        self.season.is_some() || self.episode.is_some() || self.absolute.is_some()
    }
}

/// An IMDb id as IMDb writes it, or None.
fn imdb(raw: &str) -> Option<String> {
    let id = raw.trim().to_ascii_lowercase();
    let digits = id.strip_prefix("tt")?;
    (digits.len() >= 5 && digits.len() <= 10 && digits.chars().all(|c| c.is_ascii_digit()))
        .then_some(id)
}

/// A title the page could be.
#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    /// Frog's id; None for a TMDB title nobody has opened here yet.
    pub title_id: Option<i64>,
    pub tmdb: Option<TmdbRef>,
    pub name: String,
    pub kind: String,
    pub year: Option<i32>,
    pub poster_path: Option<String>,
    pub in_library: bool,
    #[serde(skip)]
    score: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Match {
    #[serde(flatten)]
    pub title: Candidate,
    pub confidence: Confidence,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    pub episode_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Resolution {
    #[serde(rename = "match")]
    pub best: Option<Match>,
    pub alternatives: Vec<Candidate>,
}

// --- decisions on plain values -------------------------------------------------

/// Words that say how a page offers something, never what it is.
const NOISE: &[&str] = &[
    "watch",
    "online",
    "free",
    "stream",
    "streaming",
    "full",
    "hd",
    "sub",
    "dub",
    "subbed",
    "dubbed",
    "english",
    "eng",
    "1080p",
    "720p",
    "4k",
];

/// A name with what pages put around it taken out: episode numbers, a year in
/// brackets, "watch … online free". What is left is what to search for.
pub fn clean_name(raw: &str) -> String {
    let mut words: Vec<&str> = Vec::new();
    let tokens: Vec<&str> = raw
        .split(|c: char| c.is_whitespace() || c == '_' || c == '.')
        .filter(|w| !w.is_empty())
        .collect();
    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i];
        let lower = token.to_lowercase();
        let bare = lower.trim_matches(|c: char| !c.is_alphanumeric());
        let next_is_number = tokens.get(i + 1).is_some_and(|n| {
            n.trim_matches(|c: char| !c.is_alphanumeric())
                .parse::<u32>()
                .is_ok()
        });
        if matches!(
            bare,
            "season" | "episode" | "ep" | "series" | "part" | "cour"
        ) && next_is_number
        {
            i += 2;
            continue;
        }
        if is_episode_code(bare) || is_bracketed_year(token) || NOISE.contains(&bare) {
            i += 1;
            continue;
        }
        words.push(token);
        i += 1;
    }
    let joined = words.join(" ");
    joined
        .trim_matches(|c: char| c.is_whitespace() || "-–—|:·,".contains(c))
        .to_string()
}

/// The season and episode a piece of text names: S02E07, 2x07,
/// "Season 2 Episode 7", or an episode alone ("Episode 7", "Ep 7", "E07").
pub fn numbers_in(text: &str) -> Option<(Option<i64>, i64)> {
    let tokens: Vec<String> = text
        .split(|c: char| c.is_whitespace() || c == '_' || c == '.' || c == '|' || c == '-')
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_ascii_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect();
    let num = |s: &str| s.parse::<i64>().ok().filter(|n| *n <= 100_000);
    let mut season = None;
    for (i, token) in tokens.iter().enumerate() {
        let next = tokens.get(i + 1).and_then(|n| num(n));
        if let Some(rest) = token.strip_prefix('s') {
            if let Some((s, e)) = rest.split_once('e') {
                if let (Some(s), Some(e)) = (num(s), num(e)) {
                    return Some((Some(s), e));
                }
            }
        }
        if let Some((s, e)) = token.split_once('x') {
            if let (Some(s), Some(e)) = (num(s), num(e)) {
                return Some((Some(s), e));
            }
        }
        match token.as_str() {
            "season" | "series" => season = next.or(season),
            "episode" | "ep" => {
                if let Some(e) = next {
                    return Some((season, e));
                }
            }
            _ => {
                if let Some(e) = token
                    .strip_prefix("ep")
                    .or(token.strip_prefix('e'))
                    .and_then(num)
                {
                    return Some((season, e));
                }
            }
        }
    }
    None
}

/// S02E07, s2e7, 2x07, E07, EP07.
fn is_episode_code(word: &str) -> bool {
    let w = word.to_ascii_lowercase();
    let digits = |s: &str| !s.is_empty() && s.len() <= 4 && s.chars().all(|c| c.is_ascii_digit());
    if let Some(rest) = w.strip_prefix('s') {
        if let Some((s, e)) = rest.split_once('e') {
            return digits(s) && digits(e);
        }
    }
    if let Some((s, e)) = w.split_once('x') {
        return digits(s) && digits(e);
    }
    if let Some(rest) = w.strip_prefix("ep") {
        return digits(rest);
    }
    w.strip_prefix('e').is_some_and(digits)
}

fn is_bracketed_year(word: &str) -> bool {
    let inner = word.trim_matches(|c| c == '(' || c == ')' || c == '[' || c == ']');
    inner.len() == 4
        && inner != word
        && inner
            .parse::<u32>()
            .is_ok_and(|y| (1870..=2100).contains(&y))
}

/// The names worth searching for: the extension's best guess, then each part
/// of the page's title (sites put their own name after a bar or a dash).
pub fn name_candidates(title: Option<&str>, page_title: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |raw: &str| {
        let name = clean_name(raw);
        if name.chars().count() >= 2 && !out.iter().any(|o| text::fold(o) == text::fold(&name)) {
            out.push(name);
        }
    };
    if let Some(t) = title {
        push(t);
    }
    if let Some(page) = page_title {
        for part in page
            .split(['|', '–', '—', '·', '•', ':'])
            .flat_map(|p| p.split(" - "))
        {
            push(part);
        }
        push(page);
    }
    out.truncate(4);
    out
}

/// Where an episode sits in the show's own numbering, given how many episodes
/// each season holds (season 0 is specials). An episode number past the end of
/// season 1, or one with no season, is counted from the start of the show, the
/// way anime sites number them. With no episode list there is nothing to
/// check against, and the page's own numbers stand.
pub fn place_episode(
    season: Option<i64>,
    episode: Option<i64>,
    absolute: Option<i64>,
    seasons: &[(i64, i64)],
) -> Option<(i64, i64)> {
    let fits = |s: i64, e: i64| {
        seasons
            .iter()
            .any(|&(n, count)| n == s && e >= 1 && e <= count)
    };
    if seasons.is_empty() {
        return match (season, episode) {
            (Some(s), Some(e)) => Some((s, e)),
            _ => None,
        };
    }
    if let (Some(s), Some(e)) = (season, episode) {
        if fits(s, e) {
            return Some((s, e));
        }
        if s > 1 {
            return None;
        }
    }
    let counted = absolute.or(episode)?;
    let (s, offset) = locate(counted - 1, seasons)?;
    Some((s, offset + 1))
}

/// How well a title answers the page: the name first, then the year, then
/// whether it is the right kind of thing.
pub fn score(
    closeness: Closeness,
    year_wanted: Option<i32>,
    year: Option<i32>,
    episodic: bool,
    kind_wanted: Option<&str>,
    kind: &str,
    in_library: bool,
) -> i32 {
    let mut score = match closeness {
        Closeness::Exact => 60,
        Closeness::Starts => 30,
        Closeness::AllWords => 25,
        Closeness::Loose => 0,
    };
    match (year_wanted, year) {
        (Some(w), Some(y)) if w == y => score += 25,
        (Some(w), Some(y)) if (w - y).abs() == 1 => score += 10,
        (Some(_), Some(_)) => score -= 20,
        _ => {}
    }
    if kind_wanted.is_some_and(|k| k != kind) || (episodic && kind != "tv") {
        score -= 40;
    } else if episodic {
        score += 10;
    }
    if in_library {
        score += 10;
    }
    score
}

/// A name alone is never exact. It is likely when it scores well and clearly
/// ahead of the next one.
pub fn text_confidence(best: i32, second: Option<i32>) -> Confidence {
    if best >= 70 && second.is_none_or(|s| best - s >= 15) {
        Confidence::Likely
    } else {
        Confidence::Guess
    }
}

fn year_of(date: Option<&str>) -> Option<i32> {
    date.filter(|d| d.len() >= 4)
        .and_then(|d| d[..4].parse().ok())
}

// --- resolving -------------------------------------------------------------------

/// The best match for a page, and what else it could be.
pub async fn resolve(state: &AppState, user_id: i64, signals: Signals) -> AppResult<Resolution> {
    let signals = signals.checked().with_numbers_from_text();
    let mut found = by_id(state, user_id, &signals).await?;
    let mut alternatives = Vec::new();

    let names = name_candidates(signals.title.as_deref(), signals.page_title.as_deref());
    if found.is_none()
        || found
            .as_ref()
            .is_some_and(|(_, c, _)| *c < Confidence::Exact)
    {
        let mut ranked = by_name(state, user_id, &signals, &names).await?;
        if let Some((by_id_title, _, _)) = &found {
            ranked.retain(|c| !same(c, by_id_title));
        } else if let Some(first) = ranked.first().cloned() {
            let confidence = text_confidence(first.score, ranked.get(1).map(|c| c.score));
            ranked.remove(0);
            found = Some((first, confidence, None));
        }
        alternatives = ranked;
    }
    alternatives.truncate(ALTERNATIVES);

    let Some((mut title, mut confidence, numbers)) = found else {
        return Ok(Resolution {
            best: None,
            alternatives,
        });
    };

    // Only a title Frog holds has an episode list to place numbers on. The
    // best one is opened now so the extension can log it straight away.
    if title.title_id.is_none() {
        if let Some(t) = &title.tmdb {
            match catalog::ensure(state, &ExternalId::tmdb(&t.kind, t.id)).await {
                Ok((id, _)) => title.title_id = Some(id),
                Err(e) => tracing::warn!(error = %e, "could not open a match"),
            }
        }
    }

    let (mut season, mut episode, mut episode_name) = (None, None, None);
    if title.kind == "tv" {
        let (s, e) = numbers.unwrap_or((signals.season, signals.episode));
        if let Some(title_id) = title.title_id {
            let seasons = seasons_of(state, title_id).await?;
            if let Some((s, e)) = place_episode(s, e, signals.absolute, &seasons) {
                season = Some(s);
                episode = Some(e);
                episode_name = episode_name_of(&state.pool, title_id, s, e).await?;
            }
        }
        // A show is only sure once it is clear which episode is on.
        if episode.is_none() {
            confidence = confidence.min(Confidence::Likely);
        }
    }
    if title.title_id.is_none() {
        confidence = confidence.min(Confidence::Guess);
    }

    Ok(Resolution {
        best: Some(Match {
            title,
            confidence,
            season,
            episode,
            episode_name,
        }),
        alternatives,
    })
}

/// The title an id names now: a merged title's place is the one it went into.
/// None for an id Frog does not have, which a stale site rule can send.
async fn current(pool: &SqlitePool, title_id: i64) -> AppResult<Option<i64>> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT COALESCE(merged_into, id) FROM title WHERE id = ?")
            .bind(title_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|r| r.0))
}

fn same(a: &Candidate, b: &Candidate) -> bool {
    (a.title_id.is_some() && a.title_id == b.title_id)
        || (a.tmdb.is_some()
            && a.tmdb.as_ref().map(|t| (&t.kind, t.id)) == b.tmdb.as_ref().map(|t| (&t.kind, t.id)))
}

type Found = (Candidate, Confidence, Option<(Option<i64>, Option<i64>)>);

/// A title from an id on the page: Frog's own from a learned rule, TMDB's, or
/// IMDb's. Exact, unless the page gave a TMDB id without saying which kind.
async fn by_id(state: &AppState, user_id: i64, s: &Signals) -> AppResult<Option<Found>> {
    if let Some(id) = s.title_id {
        let Some(id) = current(&state.pool, id).await? else {
            return Ok(None);
        };
        if let Some(c) = local_candidate(&state.pool, user_id, id).await? {
            return Ok(Some((c, Confidence::Exact, None)));
        }
    }
    if let Some(tmdb_id) = s.tmdb_id {
        let kinds: Vec<&str> = match s.kind.as_deref() {
            Some(k) => vec![k],
            None if s.episodic() => vec!["tv"],
            None => vec!["movie", "tv"],
        };
        let sure = kinds.len() == 1;
        let mut held = Vec::new();
        for kind in &kinds {
            if let Some(id) = catalog::find(&state.pool, &ExternalId::tmdb(kind, tmdb_id)).await? {
                if let Some(c) = local_candidate(&state.pool, user_id, id).await? {
                    held.push(c);
                }
            }
        }
        // Of a film and a show with the same number, the one in the library.
        held.sort_by_key(|c| !c.in_library);
        if let Some(c) = held.into_iter().next() {
            let confidence = if sure {
                Confidence::Exact
            } else {
                Confidence::Likely
            };
            return Ok(Some((c, confidence, None)));
        }
        if state.tmdb.is_configured() {
            for kind in &kinds {
                if let Ok((id, _)) = catalog::ensure(state, &ExternalId::tmdb(kind, tmdb_id)).await
                {
                    if let Some(c) = local_candidate(&state.pool, user_id, id).await? {
                        let confidence = if sure {
                            Confidence::Exact
                        } else {
                            Confidence::Likely
                        };
                        return Ok(Some((c, confidence, None)));
                    }
                }
            }
        }
    }
    if let Some(imdb_id) = &s.imdb_id {
        if let Some(id) = catalog::find(&state.pool, &ExternalId::Imdb(imdb_id.clone())).await? {
            if let Some(c) = local_candidate(&state.pool, user_id, id).await? {
                return Ok(Some((c, Confidence::Exact, None)));
            }
        }
        if state.tmdb.is_configured() {
            return by_imdb_on_tmdb(state, user_id, imdb_id).await;
        }
    }
    Ok(None)
}

/// TMDB's answer for an IMDb id. An episode's own id names its show and its
/// numbers, which is as exact as a page can be.
async fn by_imdb_on_tmdb(
    state: &AppState,
    user_id: i64,
    imdb_id: &str,
) -> AppResult<Option<Found>> {
    let found = match state.tmdb.find(imdb_id, "imdb_id").await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "TMDB could not look up an IMDb id");
            return Ok(None);
        }
    };
    let first = |key: &str| found[key].as_array().and_then(|a| a.first()).cloned();
    let (kind, id, numbers) = if let Some(m) = first("movie_results") {
        ("movie", m["id"].as_i64(), None)
    } else if let Some(t) = first("tv_results") {
        ("tv", t["id"].as_i64(), None)
    } else if let Some(e) = first("tv_episode_results") {
        let numbers = (e["season_number"].as_i64(), e["episode_number"].as_i64());
        ("tv", e["show_id"].as_i64(), Some(numbers))
    } else {
        return Ok(None);
    };
    let Some(id) = id else { return Ok(None) };
    let Ok((title_id, _)) = catalog::ensure(state, &ExternalId::tmdb(kind, id)).await else {
        return Ok(None);
    };
    Ok(local_candidate(&state.pool, user_id, title_id)
        .await?
        .map(|c| (c, Confidence::Exact, numbers)))
}

/// (id, kind, name, release date, poster)
type TitleBrief = (i64, String, String, Option<String>, Option<String>);

async fn local_candidate(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
) -> AppResult<Option<Candidate>> {
    let row: Option<TitleBrief> = sqlx::query_as(
        "SELECT id, kind, name, release_date, poster_path FROM title
          WHERE id = ? AND kind IN ('movie', 'tv')",
    )
    .bind(title_id)
    .fetch_optional(pool)
    .await?;
    let Some((id, kind, name, date, poster)) = row else {
        return Ok(None);
    };
    let externals = catalog::externals(pool, id).await?;
    let in_library = store::get_entry(pool, user_id, id).await?.is_some();
    Ok(Some(Candidate {
        title_id: Some(id),
        tmdb: externals.tmdb,
        name,
        kind,
        year: year_of(date.as_deref()),
        poster_path: poster,
        in_library,
        score: 0,
    }))
}

/// Titles by name: what Frog already holds, the library first, then TMDB's
/// search. Best first.
async fn by_name(
    state: &AppState,
    user_id: i64,
    s: &Signals,
    names: &[String],
) -> AppResult<Vec<Candidate>> {
    let mut found: Vec<Candidate> = Vec::new();
    let episodic = s.episodic();
    let rate = |c: &Candidate, name: &str| {
        let close = text::closeness(&c.name, &text::words(name));
        let points = score(
            close,
            s.year,
            c.year,
            episodic,
            s.kind.as_deref(),
            &c.kind,
            c.in_library,
        );
        (close != Closeness::Loose).then_some(points)
    };

    for name in names {
        // A % or _ from a page is a character, never a wildcard. A title
        // written before `search_name` existed matches by its plain name
        // until the next start fills it in, as in the library filter.
        let folded = text::fold(name).replace(['%', '_'], " ");
        let rows: Vec<TitleBrief> = sqlx::query_as(
            "SELECT id, kind, name, release_date, poster_path FROM title
              WHERE merged_into IS NULL AND kind IN ('movie', 'tv')
                AND COALESCE(search_name, lower(name || char(10) || COALESCE(original_name, '')))
                    LIKE ?
              LIMIT 25",
        )
        .bind(format!("%{folded}%"))
        .fetch_all(&state.pool)
        .await?;
        for (id, ..) in rows {
            if found.iter().any(|c| c.title_id == Some(id)) {
                continue;
            }
            if let Some(mut c) = local_candidate(&state.pool, user_id, id).await? {
                if let Some(points) = rate(&c, name) {
                    c.score = points;
                    found.push(c);
                }
            }
        }
    }

    if state.tmdb.is_configured() {
        for name in names.iter().take(2) {
            let answer = match state.tmdb.search_multi(name, 1).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error = %e, "TMDB search failed for a page");
                    continue;
                }
            };
            for (rank, r) in tmdb_results(&answer).into_iter().enumerate().take(10) {
                let Some(tmdb) = r.tmdb.clone() else { continue };
                if found.iter().any(|c| {
                    c.tmdb
                        .as_ref()
                        .is_some_and(|t| t.kind == tmdb.kind && t.id == tmdb.id)
                }) {
                    continue;
                }
                let mut c = match catalog::find(&state.pool, &ExternalId::tmdb(&tmdb.kind, tmdb.id))
                    .await?
                {
                    Some(id) => match local_candidate(&state.pool, user_id, id).await? {
                        Some(c) => c,
                        None => continue,
                    },
                    None => r,
                };
                if let Some(points) = rate(&c, name) {
                    // TMDB's own order breaks a tie: its first answer is usually it.
                    c.score = points + if rank == 0 { 5 } else { 0 };
                    found.push(c);
                }
            }
        }
    }

    found.sort_by_key(|c| std::cmp::Reverse(c.score));
    Ok(found)
}

fn tmdb_results(answer: &Value) -> Vec<Candidate> {
    answer["results"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|r| {
                    let kind = r["media_type"].as_str()?;
                    if kind != "movie" && kind != "tv" {
                        return None;
                    }
                    let name = r["title"].as_str().or(r["name"].as_str())?.to_string();
                    let date = r["release_date"].as_str().or(r["first_air_date"].as_str());
                    Some(Candidate {
                        title_id: None,
                        tmdb: Some(TmdbRef {
                            kind: kind.to_string(),
                            id: r["id"].as_i64()?,
                        }),
                        name,
                        kind: kind.to_string(),
                        year: year_of(date),
                        poster_path: r["poster_path"].as_str().map(str::to_string),
                        in_library: false,
                        score: 0,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// How many episodes each season of a show holds, its list filled in first
/// when its provider can be reached.
async fn seasons_of(state: &AppState, title_id: i64) -> AppResult<Vec<(i64, i64)>> {
    if let Err(e) = catalog::ensure_episodes(state, title_id, None).await {
        tracing::warn!(error = %e, "could not list a show's episodes");
    }
    Ok(sqlx::query_as(
        "SELECT season_number, COUNT(*) FROM episode WHERE title_id = ?
          GROUP BY season_number ORDER BY season_number",
    )
    .bind(title_id)
    .fetch_all(&state.pool)
    .await?)
}

async fn episode_name_of(
    pool: &SqlitePool,
    title_id: i64,
    season: i64,
    episode: i64,
) -> AppResult<Option<String>> {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT name FROM episode WHERE title_id = ? AND season_number = ? AND episode_number = ?",
    )
    .bind(title_id)
    .bind(season)
    .bind(episode)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|r| r.0))
}

// --- logging ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LogRequest {
    pub title_id: Option<i64>,
    /// A TMDB title Frog has not opened yet, as `resolve` offered it.
    pub tmdb_id: Option<i64>,
    pub kind: Option<String>,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    /// When it was watched; now when left out. Never in the future.
    pub watched_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Logged {
    pub log_id: i64,
    /// True when this was already logged within `SAME_WATCH_HOURS`, and
    /// nothing new was written.
    pub already: bool,
    pub title_id: i64,
    pub season: Option<i64>,
    pub episode: Option<i64>,
}

/// The moment a watch is dated, from what the extension sent.
pub fn watched_at(sent: Option<&str>, now: DateTime<Utc>) -> AppResult<String> {
    let Some(raw) = sent else {
        return Ok(now.to_rfc3339());
    };
    let at = DateTime::parse_from_rfc3339(raw)
        .map_err(|_| AppError::BadRequest("a watch needs a valid time".into()))?
        .with_timezone(&Utc);
    // A queued log arrives late, never early; a clock a little ahead is forgiven.
    if at > now + Duration::minutes(5) {
        return Err(AppError::BadRequest(
            "a watch cannot be in the future".into(),
        ));
    }
    Ok(at.to_rfc3339())
}

/// Log a film or an episode, as the title page would tick it.
pub async fn log(
    state: &AppState,
    user_id: i64,
    token_id: i64,
    req: LogRequest,
) -> AppResult<Logged> {
    let now = Utc::now();
    let when = watched_at(req.watched_at.as_deref(), now)?;
    let title_id = match (req.title_id, req.tmdb_id) {
        (Some(id), _) => current(&state.pool, id)
            .await?
            .ok_or_else(|| AppError::NotFound("no such title".into()))?,
        (None, Some(tmdb_id)) => {
            let kind = req
                .kind
                .as_deref()
                .filter(|k| *k == "movie" || *k == "tv")
                .ok_or_else(|| {
                    AppError::BadRequest("a TMDB id needs its kind, movie or tv".into())
                })?;
            catalog::ensure(state, &ExternalId::tmdb(kind, tmdb_id))
                .await?
                .0
        }
        (None, None) => return Err(AppError::BadRequest("say which title was watched".into())),
    };
    let title = catalog::get_title(&state.pool, title_id).await?;

    let episode_id = match title.kind.as_str() {
        "tv" => {
            let (Some(season), Some(episode)) = (req.season, req.episode) else {
                return Err(AppError::BadRequest("say which episode was watched".into()));
            };
            catalog::ensure_episodes(state, title_id, None).await?;
            Some(store::episode_id(&state.pool, title_id, season, episode).await?)
        }
        "movie" => None,
        _ => {
            return Err(AppError::BadRequest(
                "only films and shows are logged".into(),
            ))
        }
    };

    let since = (now - Duration::hours(SAME_WATCH_HOURS)).to_rfc3339();
    let earlier: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM extension_log
          WHERE user_id = ? AND token_id = ? AND title_id = ? AND episode_id IS ?
            AND watch_id IS NOT NULL AND logged_at > ?
          ORDER BY logged_at DESC LIMIT 1",
    )
    .bind(user_id)
    .bind(token_id)
    .bind(title_id)
    .bind(episode_id)
    .bind(&since)
    .fetch_optional(&state.pool)
    .await?;
    let numbers = (
        req.season.filter(|_| episode_id.is_some()),
        req.episode.filter(|_| episode_id.is_some()),
    );
    if let Some((log_id,)) = earlier {
        return Ok(Logged {
            log_id,
            already: true,
            title_id,
            season: numbers.0,
            episode: numbers.1,
        });
    }

    let before = store::get_entry(&state.pool, user_id, title_id)
        .await?
        .map(|e| e.status);
    let watch_id = store::add_watch(
        &state.pool,
        user_id,
        title_id,
        episode_id,
        &when,
        false,
        "extension",
    )
    .await?;
    if title.kind == "movie" {
        // Watching a film is finishing it, as Completed on its page means.
        if before.as_deref() != Some("completed") {
            let patch = store::LibraryPatch {
                status: Some("completed".into()),
                ..Default::default()
            };
            store::apply_patch(&state.pool, user_id, title_id, &patch).await?;
        }
    } else {
        crate::release::after_watch(&state.pool, user_id, title_id).await?;
    }

    let (log_id,): (i64,) = sqlx::query_as(
        "INSERT INTO extension_log (user_id, token_id, title_id, episode_id, watch_id,
                                    status_before, logged_at)
         VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(user_id)
    .bind(token_id)
    .bind(title_id)
    .bind(episode_id)
    .bind(watch_id)
    .bind(&before)
    .bind(now.to_rfc3339())
    .fetch_one(&state.pool)
    .await?;

    Ok(Logged {
        log_id,
        already: false,
        title_id,
        season: numbers.0,
        episode: numbers.1,
    })
}

/// (title, watch, status before)
type LogRow = (i64, Option<i64>, Option<String>);

/// Take back a log: its watch goes, and the title's place in the library is
/// what it was before. Gives the title's id.
pub async fn undo(state: &AppState, user_id: i64, log_id: i64) -> AppResult<i64> {
    let row: Option<LogRow> = sqlx::query_as(
        "SELECT title_id, watch_id, status_before FROM extension_log WHERE id = ? AND user_id = ?",
    )
    .bind(log_id)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?;
    let Some((title_id, watch_id, before)) = row else {
        return Err(AppError::NotFound("nothing to undo".into()));
    };
    if let Some(watch_id) = watch_id {
        store::remove_watch(&state.pool, user_id, watch_id).await?;
    }
    sqlx::query("DELETE FROM extension_log WHERE id = ? AND user_id = ?")
        .bind(log_id)
        .bind(user_id)
        .execute(&state.pool)
        .await?;

    match before {
        // It came into the library with this log; with nothing else watched
        // it leaves again.
        None => {
            if !store::has_watches(&state.pool, user_id, title_id).await?
                && store::get_entry(&state.pool, user_id, title_id)
                    .await?
                    .is_some()
            {
                store::remove_entry(&state.pool, user_id, title_id).await?;
            }
        }
        Some(status) => {
            let patch = store::LibraryPatch {
                status: Some(status),
                ..Default::default()
            };
            store::apply_patch(&state.pool, user_id, title_id, &patch).await?;
            crate::release::reconcile(&state.pool, user_id, title_id).await?;
        }
    }
    Ok(title_id)
}

// --- the popup -------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct Watching {
    pub title_id: i64,
    pub name: String,
    pub poster_path: Option<String>,
    pub next: Option<store::NextEpisode>,
}

/// (id, name, poster)
type WatchingRow = (i64, String, Option<String>);

/// The shows being watched, most recently moved first, each with what is next.
pub async fn watching(pool: &SqlitePool, user_id: i64) -> AppResult<Vec<Watching>> {
    let rows: Vec<WatchingRow> = sqlx::query_as(
        "SELECT t.id, t.name, t.poster_path
           FROM library_entry l JOIN title t ON t.id = l.title_id
          WHERE l.user_id = ? AND l.status = 'watching' AND t.kind = 'tv'
          ORDER BY l.updated_at DESC
          LIMIT 30",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for (title_id, name, poster_path) in rows {
        out.push(Watching {
            title_id,
            name,
            poster_path,
            next: store::next_episode(pool, user_id, title_id).await?,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_title_loses_its_episode_and_its_site() {
        assert_eq!(
            clean_name("Severance S02E07 - Watch online free"),
            "Severance"
        );
        assert_eq!(clean_name("The Bear (2022) Season 2 Episode 7"), "The Bear");
        assert_eq!(clean_name("Frieren 1x07 HD"), "Frieren");
        assert_eq!(clean_name("Dandadan Episode 7 English Sub"), "Dandadan");
        assert_eq!(clean_name("Ep 12 – Mushishi"), "Mushishi");
        assert_eq!(clean_name("Blade Runner 2049"), "Blade Runner 2049");
        assert_eq!(clean_name("Breaking.Bad.S01E01.720p"), "Breaking Bad");
    }

    #[test]
    fn names_come_from_the_guess_then_each_part_of_the_page_title() {
        let names = name_candidates(
            Some("Severance"),
            Some("Severance S2 E7 | StreamSite - Watch online"),
        );
        assert_eq!(names[0], "Severance");
        assert!(names.contains(&"StreamSite".to_string()));
        assert_eq!(names.iter().filter(|n| *n == "Severance").count(), 1);
        assert!(name_candidates(None, None).is_empty());
    }

    #[test]
    fn numbers_are_read_from_text_in_the_ways_sites_write_them() {
        assert_eq!(numbers_in("Severance S02E07 - Watch"), Some((Some(2), 7)));
        assert_eq!(numbers_in("the.bear.2x07.720p"), Some((Some(2), 7)));
        assert_eq!(
            numbers_in("The Bear Season 2 Episode 7"),
            Some((Some(2), 7))
        );
        assert_eq!(numbers_in("Frieren Episode 28"), Some((None, 28)));
        assert_eq!(numbers_in("Ep 12 | Mushishi"), Some((None, 12)));
        assert_eq!(numbers_in("Frieren E07"), Some((None, 7)));
        assert_eq!(numbers_in("Blade Runner 2049"), None);
        assert_eq!(numbers_in("Sex Education"), None);
    }

    #[test]
    fn episode_codes_are_recognised_and_ordinary_words_are_not() {
        for code in ["S02E07", "s2e7", "2x07", "E07", "ep7"] {
            assert!(is_episode_code(code), "{code}");
        }
        for word in ["Sex", "Seven", "Ex", "Xena", "Episode"] {
            assert!(!is_episode_code(word), "{word}");
        }
    }

    const SHOW: [(i64, i64); 4] = [(0, 3), (1, 12), (2, 12), (3, 24)];

    #[test]
    fn numbers_that_fit_stand() {
        assert_eq!(place_episode(Some(2), Some(7), None, &SHOW), Some((2, 7)));
        assert_eq!(place_episode(Some(0), Some(2), None, &SHOW), Some((0, 2)));
    }

    #[test]
    fn an_absolute_number_walks_the_seasons() {
        assert_eq!(place_episode(None, None, Some(13), &SHOW), Some((2, 1)));
        assert_eq!(place_episode(None, None, Some(30), &SHOW), Some((3, 6)));
        assert_eq!(place_episode(None, Some(5), None, &SHOW), Some((1, 5)));
        // "Season 1 Episode 30" on a site that never splits its seasons.
        assert_eq!(place_episode(Some(1), Some(30), None, &SHOW), Some((3, 6)));
        assert_eq!(place_episode(None, None, Some(49), &SHOW), None);
    }

    #[test]
    fn a_later_season_that_does_not_fit_is_not_guessed() {
        assert_eq!(place_episode(Some(2), Some(30), None, &SHOW), None);
    }

    #[test]
    fn with_no_episode_list_the_page_is_taken_at_its_word() {
        assert_eq!(place_episode(Some(4), Some(2), None, &[]), Some((4, 2)));
        assert_eq!(place_episode(None, None, Some(30), &[]), None);
    }

    #[test]
    fn the_name_counts_most_then_the_year_then_the_kind() {
        let exact = score(
            Closeness::Exact,
            Some(2019),
            Some(2019),
            true,
            None,
            "tv",
            false,
        );
        let wrong_year = score(
            Closeness::Exact,
            Some(2019),
            Some(2008),
            true,
            None,
            "tv",
            false,
        );
        let a_film = score(
            Closeness::Exact,
            Some(2019),
            Some(2019),
            true,
            None,
            "movie",
            false,
        );
        let partly = score(
            Closeness::Starts,
            Some(2019),
            Some(2019),
            true,
            None,
            "tv",
            false,
        );
        assert!(exact > wrong_year && exact > a_film && exact > partly);
        assert!(
            score(Closeness::Exact, None, None, false, None, "movie", true)
                > score(Closeness::Exact, None, None, false, None, "movie", false)
        );
    }

    #[test]
    fn a_name_is_likely_only_when_it_stands_out() {
        assert_eq!(text_confidence(95, None), Confidence::Likely);
        assert_eq!(text_confidence(95, Some(60)), Confidence::Likely);
        assert_eq!(text_confidence(95, Some(90)), Confidence::Guess);
        assert_eq!(text_confidence(40, None), Confidence::Guess);
        assert!(Confidence::Exact > Confidence::Likely && Confidence::Likely > Confidence::Guess);
    }

    #[test]
    fn what_cannot_be_right_is_dropped() {
        let s = Signals {
            imdb_id: Some("TT0903747".into()),
            kind: Some("anime".into()),
            season: Some(-1),
            episode: Some(0),
            year: Some(3000),
            title: Some("   ".into()),
            ..Default::default()
        }
        .checked();
        assert_eq!(s.imdb_id.as_deref(), Some("tt0903747"));
        assert!(s.kind.is_none() && s.season.is_none() && s.episode.is_none());
        assert!(s.year.is_none() && s.title.is_none());
        assert!(Signals {
            imdb_id: Some("tt12".into()),
            ..Default::default()
        }
        .checked()
        .imdb_id
        .is_none());
    }

    #[test]
    fn ids_are_not_held_to_episode_counts() {
        let s = Signals {
            tmdb_id: Some(247_718),
            title_id: Some(1_061_474),
            episode: Some(247_718),
            ..Default::default()
        }
        .checked();
        assert_eq!(s.tmdb_id, Some(247_718), "a show from 2025");
        assert_eq!(s.title_id, Some(1_061_474));
        assert!(s.episode.is_none(), "no episode is numbered that high");
        let bad = Signals {
            tmdb_id: Some(0),
            title_id: Some(i64::MAX),
            ..Default::default()
        }
        .checked();
        assert!(bad.tmdb_id.is_none() && bad.title_id.is_none());
    }

    #[test]
    fn a_watch_is_never_dated_ahead() {
        let now = Utc::now();
        assert!(watched_at(None, now).is_ok());
        let later = (now + Duration::hours(2)).to_rfc3339();
        assert!(watched_at(Some(&later), now).is_err());
        let earlier = (now - Duration::hours(2)).to_rfc3339();
        assert!(watched_at(Some(&earlier), now).is_ok());
        assert!(watched_at(Some("yesterday"), now).is_err());
    }
}
