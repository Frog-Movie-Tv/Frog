//! Talking to AniList: sign-in, the GraphQL calls (paced, retried, and
//! read strictly so half an answer is never taken for an empty list), and
//! AniList's list entries as plain values.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use serde::Serialize;
use serde_json::{json, Value};

use crate::config::AniListClient;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// AniList allows 30 requests a minute at the moment; stay under it.
const SPACING: Duration = Duration::from_millis(2_100);

/// Tries of one request before it counts as failed.
const ATTEMPTS: u32 = 4;

/// AniList hands out a list 500 entries at a time at most.
const PER_CHUNK: i64 = 500;

/// Lists longer than this many chunks are not read; nobody has 20,000.
const MAX_CHUNKS: i64 = 40;

const SIGNED_OUT: &str = "AniList signed Frog out. Connect it again in Settings.";

static GATE: tokio::sync::Mutex<Option<Instant>> = tokio::sync::Mutex::const_new(None);

pub struct Api<'a> {
    pub http: &'a reqwest::Client,
    pub client: &'a AniListClient,
}

/// AniList's public API as an instance without its own client reaches it:
/// enough for reads that need nobody's token.
static PUBLIC: std::sync::OnceLock<AniListClient> = std::sync::OnceLock::new();

impl<'a> Api<'a> {
    /// For public reads such as air times, which every instance can make,
    /// signed-in AniList or not. Paced with every other call.
    pub fn public(state: &'a AppState) -> Self {
        let client = state.config.anilist.as_ref().unwrap_or_else(|| {
            PUBLIC.get_or_init(|| AniListClient {
                id: String::new(),
                secret: String::new(),
                redirect_uri: String::new(),
                api_url: "https://graphql.anilist.co".into(),
                site_url: "https://anilist.co".into(),
                anime_ids_url: crate::config::ANIME_IDS_URL.into(),
                anime_lists_url: crate::config::ANIME_LISTS_URL.into(),
            })
        });
        Self {
            http: &state.http,
            client,
        }
    }

    pub fn new(state: &'a AppState) -> AppResult<Self> {
        let client = state
            .config
            .anilist
            .as_ref()
            .ok_or_else(|| AppError::NotFound("AniList is not set up on this Frog.".into()))?;
        Ok(Self {
            http: &state.http,
            client,
        })
    }

    fn is_live(&self) -> bool {
        self.client
            .api_url
            .starts_with("https://graphql.anilist.co")
    }

    pub fn authorize_url(&self, state: &str) -> String {
        format!(
            "{}/api/v2/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&state={}",
            self.client.site_url,
            urlencoding::encode(&self.client.id),
            urlencoding::encode(&self.client.redirect_uri),
            urlencoding::encode(state),
        )
    }

    /// Trade the code from the sign-in redirect for a token and its lifetime.
    pub async fn exchange(&self, code: &str) -> AppResult<(String, i64)> {
        let resp = self
            .http
            .post(format!("{}/api/v2/oauth/token", self.client.site_url))
            .header("accept", "application/json")
            .json(&json!({
                "grant_type": "authorization_code",
                "client_id": self.client.id,
                "client_secret": self.client.secret,
                "redirect_uri": self.client.redirect_uri,
                "code": code,
            }))
            .send()
            .await
            .map_err(|e| AppError::AniList(e.to_string()))?;
        let status = resp.status();
        let body: Value = resp.json().await.unwrap_or(Value::Null);
        let token = body["access_token"].as_str().map(str::to_string);
        match token {
            Some(t) if status.is_success() => {
                // AniList says a year; trust what it answers when it answers.
                let seconds = body["expires_in"].as_i64().unwrap_or(365 * 24 * 3600);
                Ok((t, seconds))
            }
            _ => Err(AppError::BadRequest(
                "AniList did not accept that sign-in. Try connecting again.".into(),
            )),
        }
    }

    /// Wait before another try: doubling from a couple of seconds, with
    /// jitter so two accounts failing together do not retry in step. A
    /// stand-in AniList in tests gets milliseconds.
    async fn backoff(&self, attempt: u32) {
        let base = if self.is_live() { 2_000.0 } else { 20.0 };
        let jitter: f64 = {
            use rand::Rng;
            rand::thread_rng().gen_range(0.5..1.5)
        };
        let ms = base * f64::from(1u32 << attempt.min(5)) * jitter;
        tokio::time::sleep(Duration::from_millis(ms as u64)).await;
    }

    /// One GraphQL call, paced under AniList's limit. A rate limit waits as
    /// long as AniList asks; a dropped connection, a server error or an
    /// unreadable answer is tried again with backoff, a few times at most. A
    /// refused token is not retried: it needs the person to connect again.
    pub async fn graphql(&self, token: &str, query: &str, variables: Value) -> AppResult<Value> {
        // AniList is a source, never a destination. Every call in Frog comes
        // through here, so this one check holds for all of them.
        if !is_read(query) {
            return Err(AppError::AniList("Frog only reads from AniList".into()));
        }
        let mut last_error = String::from("no answer");
        for attempt in 0..ATTEMPTS {
            {
                let mut last = GATE.lock().await;
                if self.is_live() {
                    if let Some(at) = *last {
                        let wait = SPACING.saturating_sub(at.elapsed());
                        if !wait.is_zero() {
                            tokio::time::sleep(wait).await;
                        }
                    }
                }
                *last = Some(Instant::now());
            }

            let mut req = self.http.post(&self.client.api_url);
            // No token is a public read, such as a search.
            if !token.is_empty() {
                req = req.bearer_auth(token);
            }
            let resp = match req
                .header("accept", "application/json")
                .json(&json!({ "query": query, "variables": variables }))
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    last_error = if e.is_timeout() {
                        "timed out".into()
                    } else {
                        "could not connect".into()
                    };
                    self.backoff(attempt).await;
                    continue;
                }
            };

            let status = resp.status();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let secs = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(60)
                    .min(65);
                tracing::info!(secs, "AniList asked Frog to slow down");
                last_error = "rate limited".into();
                if attempt + 1 < ATTEMPTS {
                    tokio::time::sleep(Duration::from_secs(secs)).await;
                }
                continue;
            }
            if status == reqwest::StatusCode::UNAUTHORIZED {
                return Err(AppError::Forbidden(SIGNED_OUT.into()));
            }
            if status.is_server_error() {
                last_error = status.to_string();
                self.backoff(attempt).await;
                continue;
            }
            let body: Value = match resp.json().await {
                Ok(body) => body,
                Err(_) => {
                    // Half an answer is no answer: never read as an empty list.
                    last_error = format!("{status}: unreadable answer");
                    self.backoff(attempt).await;
                    continue;
                }
            };
            if let Some(err) = body["errors"].as_array().and_then(|a| a.first()) {
                let message = err["message"].as_str().unwrap_or("unknown error");
                return Err(answer_error(status, message));
            }
            if !status.is_success() {
                return Err(AppError::AniList(status.to_string()));
            }
            if body["data"].is_null() {
                last_error = format!("{status}: no data");
                self.backoff(attempt).await;
                continue;
            }
            return Ok(body["data"].clone());
        }
        Err(AppError::AniList(last_error))
    }

    pub async fn viewer(&self, token: &str) -> AppResult<(i64, String)> {
        let data = self
            .graphql(token, "query { Viewer { id name } }", json!({}))
            .await?;
        let id = data["Viewer"]["id"]
            .as_i64()
            .ok_or_else(|| AppError::AniList("no viewer".into()))?;
        let name = data["Viewer"]["name"].as_str().unwrap_or("").to_string();
        Ok((id, name))
    }
}

/// What an error AniList answered with means for Frog. A title AniList does
/// not have, because it was removed or is kept from view, comes back as 404
/// "Not Found": that is about the one title, and must not read as AniList
/// being down, which would hold up a whole sync for one entry, every time.
fn answer_error(status: reqwest::StatusCode, message: &str) -> AppError {
    if message.to_lowercase().contains("invalid token") {
        return AppError::Forbidden(SIGNED_OUT.into());
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        return AppError::NotFound(format!("AniList has no such entry: {message}"));
    }
    AppError::AniList(format!("{status}: {message}"))
}

const LIST_QUERY: &str = "query ($userId: Int, $type: MediaType, $chunk: Int, $perChunk: Int) {
  MediaListCollection(userId: $userId, type: $type, chunk: $chunk, perChunk: $perChunk) {
    hasNextChunk
    lists { entries {
      status
      score(format: POINT_100)
      progress
      repeat
      startedAt { year month day }
      completedAt { year month day }
      media {
        id type format status episodes chapters seasonYear countryOfOrigin synonyms
        startDate { year month day }
        nextAiringEpisode { episode airingAt }
        title { english romaji native }
      }
    } }
  }
}";

pub(super) const MEDIA_QUERY: &str = "query ($ids: [Int]) {
  Page(perPage: 50) { media(id_in: $ids, type: ANIME) { id format episodes title { english romaji } } }
}";

/// One entry as AniList has it.
#[derive(Debug, Clone)]
pub struct Remote {
    pub media_id: i64,
    pub name: String,
    /// Every title AniList gives it (English, romaji, native), for matching
    /// by name when no list knows the id.
    pub titles: Vec<String>,
    /// Other names it goes by, for comparing against TMDB's.
    pub synonyms: Vec<String>,
    pub format: Option<String>,
    /// A manga: Frog keeps it as AniList's own title and never places it on
    /// TMDB. Its `episodes` are chapters, and its progress counts them.
    pub manga: bool,
    pub episodes: Option<i64>,
    pub year: Option<i64>,
    /// yyyy-mm-dd, when AniList knows the whole date.
    pub start: Option<String>,
    /// FINISHED, RELEASING, NOT_YET_RELEASED, CANCELLED or HIATUS.
    pub media_status: Option<String>,
    pub country: Option<String>,
    /// (episode, unix time) of the next episode to air.
    pub next_airing: Option<(i64, i64)>,
    pub view: View,
    /// When the person started and finished it, yyyy-mm-dd, as far as their
    /// list says.
    pub started: Option<String>,
    pub completed: Option<String>,
    /// How many times they watched it again.
    pub repeat: i64,
}

/// The three things that sync: list status (in AniList's words), how many
/// episodes in, and a 0–100 score where 0 is none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct View {
    pub status: String,
    pub progress: i64,
    pub score: i64,
}

pub(super) fn media_name(media: &Value) -> String {
    media["title"]["english"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| media["title"]["romaji"].as_str())
        .unwrap_or("Untitled")
        .to_string()
}

/// A full yyyy-mm-dd from AniList's fuzzy date, or nothing.
pub(super) fn full_date(d: &Value) -> Option<String> {
    let (y, m, day) = (
        d["year"].as_i64()?,
        d["month"].as_i64()?,
        d["day"].as_i64()?,
    );
    NaiveDate::from_ymd_opt(y as i32, m as u32, day as u32)
        .map(|d| d.format("%Y-%m-%d").to_string())
}

/// Every title AniList gives a media, in order of usefulness.
pub(super) fn titles_of(media: &Value) -> Vec<String> {
    let mut titles: Vec<String> = Vec::new();
    for key in ["english", "romaji", "native"] {
        if let Some(t) = media["title"][key].as_str().map(str::trim) {
            if !t.is_empty() && !titles.iter().any(|known| known == t) {
                titles.push(t.to_string());
            }
        }
    }
    titles
}

/// A media as a `Remote` with nothing of anyone's list in it: for placing a
/// title someone opened rather than one on their list.
pub fn remote_of_media(media: &Value) -> Remote {
    let manga = media["type"].as_str() == Some("MANGA");
    Remote {
        media_id: media["id"].as_i64().unwrap_or(0),
        name: media_name(media),
        titles: titles_of(media),
        synonyms: Vec::new(),
        format: media["format"].as_str().map(str::to_string),
        manga,
        episodes: media[if manga { "chapters" } else { "episodes" }]
            .as_i64()
            .filter(|n| *n > 0),
        year: media["startDate"]["year"].as_i64(),
        start: full_date(&media["startDate"]),
        media_status: media["status"].as_str().map(str::to_string),
        country: media["countryOfOrigin"].as_str().map(str::to_string),
        next_airing: None,
        view: View {
            status: "PLANNING".into(),
            progress: 0,
            score: 0,
        },
        started: None,
        completed: None,
        repeat: 0,
    }
}

pub fn parse_list(data: &Value) -> Vec<Remote> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let lists = data["MediaListCollection"]["lists"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for list in lists {
        for e in list["entries"].as_array().cloned().unwrap_or_default() {
            let media = &e["media"];
            let Some(media_id) = media["id"].as_i64() else {
                continue;
            };
            // Custom lists repeat entries that are already in a status list.
            if !seen.insert(media_id) {
                continue;
            }
            let status = e["status"].as_str().unwrap_or("PLANNING");
            let manga = media["type"].as_str() == Some("MANGA");
            out.push(Remote {
                media_id,
                name: media_name(media),
                titles: titles_of(media),
                synonyms: media["synonyms"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect(),
                format: media["format"].as_str().map(str::to_string),
                manga,
                episodes: media[if manga { "chapters" } else { "episodes" }]
                    .as_i64()
                    .filter(|n| *n > 0),
                // The year it started, which for a film is the year it came
                // out; seasonYear is missing for most films.
                year: media["startDate"]["year"]
                    .as_i64()
                    .or_else(|| media["seasonYear"].as_i64()),
                start: full_date(&media["startDate"]),
                media_status: media["status"].as_str().map(str::to_string),
                country: media["countryOfOrigin"].as_str().map(str::to_string),
                next_airing: media["nextAiringEpisode"]["episode"]
                    .as_i64()
                    .zip(media["nextAiringEpisode"]["airingAt"].as_i64()),
                view: View {
                    // A rewatch is still watching, as far as Frog is concerned.
                    status: if status == "REPEATING" {
                        "CURRENT".into()
                    } else {
                        status.to_string()
                    },
                    progress: e["progress"].as_i64().unwrap_or(0).max(0),
                    score: e["score"].as_f64().unwrap_or(0.0).round() as i64,
                },
                started: full_date(&e["startedAt"]),
                completed: full_date(&e["completedAt"]),
                repeat: e["repeat"].as_i64().unwrap_or(0).max(0),
            });
        }
    }
    out
}

/// One account's whole list, anime and then manga, read with its own token.
pub(super) async fn read_list(
    api: &Api<'_>,
    token: &str,
    anilist_user_id: i64,
) -> AppResult<Vec<Remote>> {
    let mut all = Vec::new();
    for kind in ["ANIME", "MANGA"] {
        let who = json!({ "userId": anilist_user_id, "type": kind });
        all.extend(fetch_list(api, token, LIST_QUERY, who).await?);
    }
    Ok(all)
}

/// A whole list, chunk by chunk. Any chunk that fails fails the lot: a list
/// read halfway would look like entries removed.
async fn fetch_list(api: &Api<'_>, token: &str, query: &str, who: Value) -> AppResult<Vec<Remote>> {
    let mut lists: Vec<Value> = Vec::new();
    for chunk in 1..=MAX_CHUNKS {
        let mut vars = who.clone();
        vars["chunk"] = json!(chunk);
        vars["perChunk"] = json!(PER_CHUNK);
        let data = api.graphql(token, query, vars).await?;
        let collection = &data["MediaListCollection"];
        if !collection["lists"].is_array() {
            return Err(AppError::AniList(
                "the list came back without entries".into(),
            ));
        }
        lists.extend(collection["lists"].as_array().cloned().unwrap_or_default());
        if collection["hasNextChunk"].as_bool() != Some(true) {
            return Ok(parse_list(
                &json!({ "MediaListCollection": { "lists": lists } }),
            ));
        }
    }
    Err(AppError::AniList(
        "the list is longer than Frog reads".into(),
    ))
}

/// Somebody's anime list by name, read anonymously. Public lists only.
pub async fn public_list(state: &AppState, user_name: &str) -> AppResult<Vec<Remote>> {
    let api = Api::new(state)?;
    let query = LIST_QUERY
        .replace("$userId: Int", "$userName: String")
        .replace("userId: $userId", "userName: $userName");
    fetch_list(
        &api,
        "",
        &query,
        json!({ "userName": user_name, "type": "ANIME" }),
    )
    .await
}

/// Whether a GraphQL document only reads. One with no operation word is a
/// query by GraphQL's own rules.
fn is_read(document: &str) -> bool {
    let first = document
        .trim_start()
        .split(|c: char| !c.is_alphanumeric())
        .next();
    matches!(first, Some("query") | Some(""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_reads_reach_anilist() {
        assert!(is_read("query ($id: Int) { Media(id: $id) { id } }"));
        assert!(is_read("  query { Viewer { id } }"));
        assert!(is_read("{ Viewer { id } }"));
        assert!(!is_read(
            "mutation { SaveMediaListEntry(mediaId: 1) { id } }"
        ));
        assert!(!is_read(
            "  mutation ($id: Int) { DeleteMediaListEntry(id: $id) { deleted } }"
        ));
        assert!(!is_read("subscription { x }"));
    }

    #[test]
    fn a_rewatch_reads_as_watching_and_duplicates_are_dropped() {
        let data = json!({ "MediaListCollection": { "lists": [
            { "entries": [
                { "status": "REPEATING", "score": 85.0, "progress": 3,
                  "media": { "id": 1, "format": "TV", "episodes": 12,
                             "title": { "english": null, "romaji": "Kimi" } } }
            ] },
            { "entries": [
                { "status": "CURRENT", "score": 85.0, "progress": 3,
                  "media": { "id": 1, "format": "TV", "episodes": 12,
                             "title": { "english": null, "romaji": "Kimi" } } }
            ] }
        ] } });
        let list = parse_list(&data);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].view.status, "CURRENT");
        assert_eq!(list[0].name, "Kimi");
        assert_eq!(list[0].view.score, 85);
    }

    #[test]
    fn a_title_anilist_does_not_have_is_not_an_outage() {
        use reqwest::StatusCode;
        assert!(matches!(
            answer_error(StatusCode::NOT_FOUND, "Not Found."),
            AppError::NotFound(_)
        ));
        assert!(matches!(
            answer_error(StatusCode::BAD_REQUEST, "Invalid token"),
            AppError::Forbidden(_)
        ));
        assert!(matches!(
            answer_error(StatusCode::BAD_REQUEST, "Validation"),
            AppError::AniList(_)
        ));
    }
}
