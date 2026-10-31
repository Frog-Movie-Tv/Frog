//! Which TMDB title an imported row means.
//!
//! Ids win: a TMDB id, then an IMDb id. Otherwise each of the row's names is
//! searched until one finds something of the right kind, and the results
//! are judged by year, by spelling and by season. Whatever is in doubt says
//! why in `note`, for the person to settle in the wizard.

use std::collections::HashMap;

use chrono::Duration;
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::tmdb::models::{self, SearchItem, SeasonSummary};

/// TMDB's genre id for Animation.
const ANIMATION: i64 = 16;

/// Rows looked up side by side within a batch: enough that a big import does
/// not wait on TMDB one row at a time, few enough to stay well under its limit.
const MATCH_AT_ONCE: usize = 6;

/// A row as it came out of someone else's export.
#[derive(Debug, Clone, Deserialize)]
pub struct Incoming {
    /// Stable id for the wizard to correlate previews with choices.
    pub key: String,
    pub title: String,
    pub year: Option<i32>,
    pub imdb_id: Option<String>,
    pub tmdb_id: Option<i64>,
    /// AniList's id, from a Frog backup of an anime TMDB lacks.
    #[serde(default)]
    pub anilist_id: Option<i64>,
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

/// Match a batch of rows for one account: what each means on TMDB, and
/// whether the account already tracks it.
pub async fn match_rows(
    state: &AppState,
    user_id: i64,
    rows: Vec<Incoming>,
) -> AppResult<Vec<MatchResult>> {
    // Every provider id of every title in the library, with its status.
    let tracked: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT x.provider, x.external_id, l.status
           FROM library_entry l JOIN title_external x ON x.title_id = l.title_id
          WHERE l.user_id = ?",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;

    // `buffered` keeps the rows in the order they came.
    Ok(stream::iter(rows)
        .map(|row| match_row(state, &tracked, row))
        .buffered(MATCH_AT_ONCE)
        .collect()
        .await)
}

/// Everything the wizard needs to know about one row.
async fn match_row(
    state: &AppState,
    tracked: &[(String, String, String)],
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

    let show = matched
        .as_ref()
        .filter(|m| m.kind == "tv")
        .and_then(|m| m.tmdb_id);
    if let Some(show) = show {
        if let Ok(detail) = state.tmdb.tv(show).await {
            let seasons = models::seasons(&detail);
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
                placed = find_episodes(state, show, &row, &seasons, note.is_some()).await;
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

    let already = matched
        .as_ref()
        .and_then(SearchItem::external)
        .and_then(|x| {
            let (provider, key) = (x.provider(), x.key());
            tracked
                .iter()
                .find(|(p, k, _)| p == provider && *k == key)
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
    seasons: &[SeasonSummary],
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
#[derive(Debug, Default)]
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

    if let Some(anilist_id) = row.anilist_id.filter(|id| *id > 0) {
        if let Ok(found) = crate::anilist::media::search_item(state, anilist_id).await {
            return Resolved::exact(found);
        }
    }

    let (mut found, searched) = match search(state, row).await {
        Ok(hit) => hit,
        Err(note) => return Resolved::missing(note),
    };
    order(&mut found, row);
    // Two shows called "One Piece", two called "Avatar: The Last Airbender":
    // the one meant has the season the row is about.
    let by_season = pick_by_season(state, &found, &searched, row).await;
    choose(found, row, &searched, by_season)
}

/// Search by each of the row's names in turn: English, romaji and synonyms
/// often differ, and the first name that finds anything of the right kind
/// wins. Gives the results and the name that found them, or why there are
/// none.
async fn search(
    state: &AppState,
    row: &Incoming,
) -> Result<(Vec<SearchItem>, String), &'static str> {
    let names: Vec<&str> = std::iter::once(row.title.as_str())
        .chain(row.alt_titles.iter().map(String::as_str))
        .filter(|n| !n.trim().is_empty())
        .collect();

    for name in names {
        let raw = match state.tmdb.search_multi(name, 1).await {
            Ok(v) => v,
            Err(_) => return Err("TMDB did not answer"),
        };
        let mut found: Vec<SearchItem> = raw
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
            return Ok((found, name.to_string()));
        }
    }
    Err("No match found")
}

/// Put the likelier results first. A live-action remake shares a name with
/// half of anime, so rows from an anime tracker see animation first; a row
/// that prefers a kind sees that kind first.
fn order(found: &mut [SearchItem], row: &Incoming) {
    if row.anime {
        found.sort_by_key(|f| !f.genre_ids.contains(&ANIMATION));
    }
    if let Some(prefer) = row.prefer.as_deref() {
        found.sort_by_key(|f| f.kind != prefer);
    }
}

/// Pick the best of the ordered results, and say when it is a guess.
///
/// The show with the row's season (`by_season`) wins outright. Otherwise an
/// exact title in the right year: a year match is what separates the 1978
/// film from the 2008 remake. Then the same letters in the same case, then
/// the same letters, then whatever came first.
fn choose(
    mut found: Vec<SearchItem>,
    row: &Incoming,
    searched: &str,
    by_season: Option<usize>,
) -> Resolved {
    let other_name = (searched != row.title).then(|| searched.to_string());
    let wanted = searched.trim().to_lowercase();
    let without = |mut found: Vec<SearchItem>, i: usize| {
        let best = found.remove(i);
        (best, found.into_iter().take(4).collect::<Vec<_>>())
    };

    let exact_same_year = found
        .iter()
        .position(|f| f.name.to_lowercase() == wanted && row.year.is_some() && f.year == row.year);
    if let Some(i) = by_season.or(exact_same_year) {
        let (best, candidates) = without(found, i);
        return Resolved {
            matched: Some(best),
            candidates,
            note: None,
            searched: other_name,
        };
    }

    // A row that prefers a kind means that kind whenever it has this name, even
    // written differently: TMDB's film is "Monsters vs Aliens", and the show is
    // the one with the full stop Netflix happens to use.
    let letters = episode_key(searched);
    let preferred = row.prefer.as_deref().and_then(|kind| {
        found
            .iter()
            .position(|f| f.kind == kind && episode_key(&f.name) == letters)
    });
    // Netflix writes the 2023 series "ONE PIECE" and the anime "One Piece":
    // the same letters in the same case win over the same letters.
    let exact = preferred
        .or_else(|| found.iter().position(|f| f.name == searched.trim()))
        .or_else(|| found.iter().position(|f| f.name.to_lowercase() == wanted));
    let best = found.remove(exact.unwrap_or(0));

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
                .any(|f| f.kind != best.kind && episode_key(&f.name) == letters) =>
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
    let mut by_name: HashMap<String, Vec<(i64, i64)>> = HashMap::new();
    for season in seasons {
        let Ok(raw) = state.tmdb.season(tmdb_id, *season).await else {
            continue;
        };
        for e in models::episodes_of_season(&raw) {
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
    placed_by_name(&by_name, names)
}

/// Which of `names` exactly one episode in `by_name` has.
fn placed_by_name(by_name: &HashMap<String, Vec<(i64, i64)>>, names: &[String]) -> Vec<Placed> {
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
fn season_words(name: &str) -> String {
    let lower = name.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .filter(|w| !matches!(*w, "the" | "arc" | "saga"))
        .collect();
    words.join(" ")
}

/// Whether a TMDB season is the one a row means: by its name when the row
/// has one, else by number with at least as many episodes as were watched.
fn season_fits(
    season: &SeasonSummary,
    name: Option<&str>,
    number: Option<i64>,
    watched: i64,
) -> bool {
    if season.season_number < 1 {
        return false;
    }
    match (name, number) {
        (Some(target), _) => {
            let have = season_words(&season.name);
            !have.is_empty()
                && (have == target
                    || format!(" {have} ").contains(&format!(" {target} "))
                    || format!(" {target} ").contains(&format!(" {have} ")))
        }
        (None, Some(n)) => season.season_number == n && season.episode_count >= watched,
        (None, None) => false,
    }
}

/// Among shows found under the same name, the first that has the row's
/// season. None when the name is not shared or nothing fits.
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
        let Some(show) = found[i].tmdb_id else {
            continue;
        };
        let Ok(detail) = state.tmdb.tv(show).await else {
            continue;
        };
        let watched = row.progress.unwrap_or(1);
        if models::seasons(&detail)
            .iter()
            .any(|s| season_fits(s, target.as_deref(), number, watched))
        {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(extra: Value) -> Incoming {
        let mut r = json!({ "key": "k", "title": "Heat" });
        for (k, v) in extra.as_object().unwrap() {
            r[k] = v.clone();
        }
        serde_json::from_value(r).unwrap()
    }

    fn item(kind: &str, id: i64, name: &str, year: i64, genres: &[i64]) -> SearchItem {
        let date = if kind == "movie" {
            "release_date"
        } else {
            "first_air_date"
        };
        let mut v = json!({ "media_type": kind, "id": id, "genre_ids": genres });
        v[if kind == "movie" { "title" } else { "name" }] = json!(name);
        v[date] = json!(format!("{year}-01-01"));
        SearchItem::from_value(&v, None).unwrap()
    }

    fn picked(r: &Resolved) -> (Option<i64>, Option<&str>) {
        (
            r.matched.as_ref().and_then(|m| m.tmdb_id),
            r.note.as_deref(),
        )
    }

    #[test]
    fn the_same_year_wins_and_a_wrong_year_is_pointed_out() {
        let found = || {
            vec![
                item("movie", 1, "Heat", 1986, &[]),
                item("movie", 2, "Heat", 1995, &[]),
            ]
        };
        let r = choose(found(), &row(json!({ "year": 1995 })), "Heat", None);
        assert_eq!(picked(&r), (Some(2), None));
        assert_eq!(r.candidates.len(), 1);

        let r = choose(found(), &row(json!({ "year": 2005 })), "Heat", None);
        assert_eq!(
            picked(&r),
            (Some(1), Some("Your file says 2005, TMDB says 1986"))
        );

        // A year a season or so out is no reason to doubt.
        let r = choose(found(), &row(json!({ "year": 1987 })), "Heat", None);
        assert_eq!(picked(&r), (Some(1), None));
        // A later season starts later than the show did.
        let r = choose(
            found(),
            &row(json!({ "year": 2005, "season": 3 })),
            "Heat",
            None,
        );
        assert_eq!(picked(&r), (Some(1), None));
    }

    #[test]
    fn the_same_casing_beats_the_same_letters_and_a_near_name_is_a_guess() {
        let found = vec![
            item("tv", 1, "ONE PIECE", 2023, &[]),
            item("tv", 2, "One Piece", 1999, &[16]),
        ];
        let r = choose(
            found,
            &row(json!({ "title": "One Piece" })),
            "One Piece",
            None,
        );
        assert_eq!(picked(&r), (Some(2), None));

        let found = vec![item("movie", 3, "Heat Wave", 2001, &[])];
        let r = choose(found, &row(json!({})), "Heat", None);
        assert_eq!(picked(&r), (Some(3), Some("Closest match by name")));
    }

    #[test]
    fn a_season_pick_wins_outright_and_another_name_is_reported() {
        let found = vec![
            item("tv", 1, "One Piece", 2023, &[]),
            item("tv", 2, "One Piece", 1999, &[]),
        ];
        let r = choose(
            found,
            &row(json!({ "title": "ワンピース", "year": 2023 })),
            "One Piece",
            Some(1),
        );
        assert_eq!(picked(&r), (Some(2), None));
        assert_eq!(r.searched.as_deref(), Some("One Piece"));
    }

    #[test]
    fn anime_rows_see_animation_first_and_a_close_call_on_kind_is_said() {
        let mut found = vec![
            item("tv", 1, "Bleach", 2025, &[18]),
            item("tv", 2, "Bleach", 2004, &[16]),
        ];
        order(&mut found, &row(json!({ "anime": true })));
        assert_eq!(found[0].tmdb_id, Some(2));

        let mut found = vec![
            item("tv", 2, "Bleach", 2004, &[16]),
            item("movie", 9, "Bleach", 2018, &[]),
        ];
        let r = row(json!({ "title": "Bleach", "prefer": "movie" }));
        order(&mut found, &r);
        let r = choose(found, &r, "Bleach", None);
        assert_eq!(
            picked(&r),
            (Some(9), Some("There is also a show by this name"))
        );
    }

    #[test]
    fn a_preferred_kind_wins_however_the_name_is_punctuated() {
        // Netflix's row for the film. TMDB writes the film without the full
        // stop and the show with it.
        let mut found = vec![
            item("tv", 60789, "Monsters vs. Aliens", 2013, &[]),
            item("movie", 15512, "Monsters vs Aliens", 2009, &[]),
        ];
        let r = row(json!({ "title": "Monsters vs. Aliens", "prefer": "movie" }));
        order(&mut found, &r);
        let r = choose(found, &r, "Monsters vs. Aliens", None);
        assert_eq!(
            picked(&r),
            (Some(15512), Some("There is also a show by this name"))
        );

        // A different name of the preferred kind is no reason to leave the
        // one spelled as asked.
        let mut found = vec![
            item("tv", 1, "Arcane", 2021, &[]),
            item("movie", 2, "Arcane Sally", 2022, &[]),
        ];
        let r = row(json!({ "title": "Arcane", "prefer": "movie" }));
        order(&mut found, &r);
        let r = choose(found, &r, "Arcane", None);
        assert_eq!(picked(&r).0, Some(1));
    }

    #[test]
    fn episode_names_place_only_when_one_episode_has_them() {
        let mut by_name: HashMap<String, Vec<(i64, i64)>> = HashMap::new();
        by_name.insert(episode_key("The Vanishing of Will Byers"), vec![(1, 1)]);
        by_name.insert(episode_key("Pilot"), vec![(1, 1), (2, 1)]);
        let names: Vec<String> = [
            "Chapter One: The Vanishing of Will Byers",
            "Pilot",
            "THE VANISHING OF WILL BYERS",
        ]
        .map(String::from)
        .to_vec();
        let placed = placed_by_name(&by_name, &names);
        let got: Vec<(usize, i64, i64)> = placed
            .iter()
            .map(|p| (p.index, p.season, p.episode))
            .collect();
        assert_eq!(got, vec![(0, 1, 1), (2, 1, 1)]);
    }

    #[test]
    fn seasons_fit_by_loose_name_or_by_number_and_length() {
        let season = |n: i64, name: &str, count: i64| SeasonSummary {
            season_number: n,
            name: name.into(),
            episode_count: count,
            poster_path: None,
            air_date: None,
            overview: None,
        };
        assert_eq!(season_words("The Egghead Arc"), "egghead");
        assert!(season_fits(
            &season(3, "Sun & Moon: Ultra Adventures", 40),
            Some("sun moon ultra adventures"),
            None,
            1
        ));
        assert!(season_fits(
            &season(21, "Egghead", 40),
            Some("egghead"),
            None,
            1
        ));
        assert!(!season_fits(
            &season(0, "Egghead", 40),
            Some("egghead"),
            None,
            1
        ));
        assert!(season_fits(&season(2, "Season 2", 12), None, Some(2), 12));
        assert!(!season_fits(&season(2, "Season 2", 12), None, Some(2), 13));
    }
}
