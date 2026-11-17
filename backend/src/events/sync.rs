//! Packs into the database, the day each opens here, and what each account
//! is told as zero comes.
//!
//! The worker runs `tick` every minute. A pack's titles are made Frog's with
//! `catalog::ensure`; one TMDB cannot give right now is asked for again ten
//! minutes on, so a release that lands while TMDB is away still fills in.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde_json::json;
use std::collections::HashSet;
use std::sync::Mutex;

use super::pack::{self, Pack};
use super::records::{self, ListRow};
use super::view;
use super::{milestone, Milestone, Phase};
use crate::catalog::{self, ExternalId};
use crate::error::AppResult;
use crate::state::AppState;

/// How often TMDB is asked again for the day a title opens.
const RELEASE_HOURS: i64 = 6;
/// How long a pack that could not be filled waits before it is tried again.
const RETRY_MINUTES: i64 = 10;

/// Packs filled this run, and when the others were last tried.
type Filled = (HashSet<String>, Option<DateTime<Utc>>);
static FILLED: Mutex<Option<Filled>> = Mutex::new(None);

fn needs_filling(slug: &str, now: DateTime<Utc>) -> bool {
    let guard = FILLED.lock().ok();
    let Some((done, tried)) = guard.as_ref().and_then(|g| g.as_ref()) else {
        return true;
    };
    !done.contains(slug) && tried.is_none_or(|t| now - t >= Duration::minutes(RETRY_MINUTES))
}

fn note_filling(slug: &str, filled: bool, now: DateTime<Utc>) {
    if let Ok(mut guard) = FILLED.lock() {
        let (done, tried) = guard.get_or_insert_with(|| (HashSet::new(), None));
        if filled {
            done.insert(slug.to_string());
        }
        *tried = Some(now);
    }
}

async fn title_of(state: &AppState, kind: &str, tmdb: i64) -> Option<i64> {
    let id = ExternalId::tmdb(kind, tmdb);
    if let Ok(Some(found)) = catalog::find(&state.pool, &id).await {
        return Some(found);
    }
    if !state.tmdb.is_configured() {
        return None;
    }
    match catalog::ensure(state, &id).await {
        Ok((title_id, _)) => Some(title_id),
        Err(e) => {
            tracing::debug!(tmdb, error = %e, "an event's title is not here yet");
            None
        }
    }
}

/// One pack into the database. Says whether everything it names is here.
pub async fn fill(state: &AppState, p: &Pack) -> AppResult<bool> {
    let target = title_of(state, &p.target.kind, p.target.tmdb).await;
    let id = records::upsert(&state.pool, &p.slug, target).await?;
    let mut rows = Vec::new();
    let mut complete = target.is_some();
    for (position, (group, e)) in p.entries().enumerate() {
        let Some(title_id) = title_of(state, &e.kind, e.tmdb).await else {
            complete = false;
            continue;
        };
        if e.kind == "tv" {
            // Readiness counts a show's episodes, so they have to be listed
            // even for someone who does not track it.
            if let Err(err) = catalog::ensure_episodes(state, title_id, None).await {
                tracing::debug!(title_id, error = %err, "an event's episodes are not listed yet");
                complete = false;
            }
        }
        rows.push(ListRow {
            position: position as i64,
            title_id,
            season_from: e.seasons.map(|s| s[0]),
            season_to: e.seasons.map(|s| s[1]),
            group_name: group.to_string(),
            essential: e.essential,
            note: e.note.clone(),
        });
    }
    records::set_titles(&state.pool, id, &rows).await?;
    Ok(complete)
}

/// The day a pack's title opens here: TMDB's for `FROG_REGION` when the pack
/// follows it and TMDB knows, else the pack's own.
async fn opens(state: &AppState, p: &Pack) -> NaiveDate {
    if !p.follows_release || !state.tmdb.is_configured() {
        return p.day;
    }
    let region = state.tmdb.region().to_string();
    let found = match (p.target.kind.as_str(), p.season) {
        ("movie", _) => state
            .tmdb
            .fetch(
                &format!("/movie/{}", p.target.tmdb),
                &[("append_to_response", "release_dates".into())],
            )
            .await
            .ok()
            .and_then(|d| crate::airing::film_opens(&d, &region))
            .map(|(day, _)| day),
        (_, Some(season)) => state
            .tmdb
            .season(p.target.tmdb, season)
            .await
            .ok()
            .and_then(|s| s["air_date"].as_str().map(str::to_string))
            .and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok()),
        _ => None,
    };
    found.unwrap_or(p.day)
}

/// The worker's minute: packs in, days checked, people told.
pub async fn tick(state: &AppState) -> AppResult<()> {
    let now = Utc::now();
    for p in pack::all() {
        if needs_filling(&p.slug, now) {
            let filled = fill(state, p).await.unwrap_or_else(|e| {
                tracing::warn!(event = %p.slug, error = %e, "an event could not be set up");
                false
            });
            note_filling(&p.slug, filled, now);
        }
        let Some(row) = records::by_slug(&state.pool, &p.slug).await? else {
            continue;
        };
        let checked = row
            .released_checked_at
            .as_deref()
            .and_then(crate::awards::eligible::parse);
        if row.release_day.is_none()
            || checked.is_none_or(|c| now - c >= Duration::hours(RELEASE_HOURS))
        {
            let day = opens(state, p).await;
            if row.day() != Some(day) {
                tracing::info!(event = %p.slug, %day, "an event opens on a new day");
            }
            records::set_release_day(&state.pool, row.id, day).await?;
        }
    }
    tell(state, now).await
}

/// What each account is told: a week to go, the day before, and zero.
pub async fn tell(state: &AppState, now: DateTime<Utc>) -> AppResult<()> {
    let pool = &state.pool;
    let events = view::known(pool, false).await?;
    if events.is_empty() {
        return Ok(());
    }
    let accounts: Vec<(i64,)> = sqlx::query_as("SELECT id FROM app_user WHERE disabled = 0")
        .fetch_all(pool)
        .await?;
    for (user_id,) in accounts {
        let tz = crate::announce::time_zone(pool, user_id).await;
        let hidden = records::hidden_ids(pool, user_id).await?;
        for k in &events {
            if hidden.contains(&k.row.id) {
                continue;
            }
            let z = k.zero_for(tz);
            let Some(m) = milestone(now, z) else {
                continue;
            };
            if m != Milestone::Out && k.phase_for(tz, now) != Phase::Live {
                continue;
            }
            let name = &k.pack.name;
            let mut text = match m {
                Milestone::Week => format!("A week to {name}"),
                Milestone::Tomorrow => format!("{name} is tomorrow"),
                Milestone::Out => format!("{name} is out"),
            };
            let mut body = match k.pack.opens.as_str() {
                "streaming" => "Streaming from today",
                "season" => "The new season starts today",
                _ => "In cinemas from today",
            }
            .to_string();
            if m != Milestone::Out {
                body = "Open the countdown".into();
                if k.pack.catch_up {
                    let rows = records::titles(pool, k.row.id).await?;
                    let r = view::readiness(&view::standings(pool, user_id, &rows).await?, now, z);
                    let left = r.essentials - r.essentials_seen;
                    if left == 0 {
                        text.push_str(" · you're ready");
                    } else {
                        text.push_str(&format!(
                            " · {left} {} left",
                            if left == 1 { "essential" } else { "essentials" }
                        ));
                        body = "Catch up before it lands".into();
                    }
                }
            }
            let title = catalog::get_title(pool, k.title_id).await?;
            let event = crate::inbox::Event {
                kind: crate::inbox::EVENT,
                title_id: Some(k.title_id),
                actor_id: None,
                payload: json!({
                    "slug": k.row.slug,
                    "text": text,
                    "body": body,
                    "milestone": m.key(),
                }),
                ref_key: Some(format!("event:{}:{}:{user_id}", k.row.slug, m.key())),
            };
            let push = json!({
                "title": text,
                "body": body,
                "url": format!("/events/{}", k.row.slug),
                "tag": format!("frog-event-{}", k.row.slug),
                "image": crate::inbox::picture(title.backdrop_path.as_deref(), title.poster_path.as_deref()),
            });
            crate::inbox::tell(state, user_id, &event, Some(push)).await?;
        }
    }
    Ok(())
}
