//! Badges Frog gives on its own.
//!
//! Two kinds, from one catalogue (`backend/badges/catalogue.json`):
//! totals count everything an account has watched, read, rated or kept,
//! however it reached Frog, history and imports included; moments (a triple
//! bill, a streak, a premiere) count only watches recorded as they happened
//! (`eligible`), and only on days that are over. The worker works each account
//! out again whenever something it counts changes or its day turns over
//! (`run`), so the rules decide from scratch every time and an award is never
//! a matter of having seen an event.
//!
//! - `catalogue`: the badges and their art.
//! - `eligible`: which watches may earn a moment. Pure.
//! - `facts`: what the rules read about one account.
//! - `rules`: what each badge asks. Pure.
//! - `give`: giving, the grace day, taking back, the notification.
//! - `custom`: badges made here that are earned by reaching a count.
//! - `art`: pictures drawn from the SVGs, never stored.

pub mod art;
pub mod catalogue;
pub mod custom;
pub mod eligible;
pub mod facts;
pub mod give;
pub mod rules;

use crate::clock::Stamp;
use chrono::Utc;
use sqlx::SqlitePool;
use std::collections::HashMap;

use crate::error::AppResult;
use crate::state::AppState;

/// Where an account's last run is marked: what it counted, and the day.
const SEEN_KEY: &str = "badges_seen";

/// How far each account is along every badge, as the worker last worked it
/// out, under the fingerprint it was worked out for. Reading the Badges tab
/// then costs a fingerprint, not the whole history (about 2 s on a long
/// one). Kept in memory: after a restart the worker's next pass fills it.
type Progress = HashMap<String, (f64, f64)>;
static PROGRESS: std::sync::Mutex<Option<HashMap<i64, (String, Progress)>>> =
    std::sync::Mutex::new(None);

fn cached(user_id: i64, mark: &str) -> Option<Progress> {
    let guard = PROGRESS.lock().ok()?;
    let (seen, map) = guard.as_ref()?.get(&user_id)?;
    (seen == mark).then(|| map.clone())
}

fn keep(user_id: i64, mark: String, map: Progress) {
    if let Ok(mut guard) = PROGRESS.lock() {
        guard
            .get_or_insert_with(HashMap::new)
            .insert(user_id, (mark, map));
    }
}

/// Everything a badge counts, cheaply, plus the day in the account's zone and
/// the catalogue: when this is what the last run saw, nothing can have
/// changed. Other accounts (taste, Pioneer) are only looked at again when the
/// day turns over.
async fn fingerprint(pool: &SqlitePool, user_id: i64, today: &str) -> AppResult<String> {
    let (counted,): (String,) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) || ':' || COALESCE(MAX(id), 0) || ':'
                        || COALESCE(TOTAL(julianday(watched_at)), 0)
                   FROM watch_event WHERE user_id = ?1)
                || '|' || (SELECT COUNT(*) || ':' || COALESCE(MAX(updated_at), '')
                             FROM library_entry WHERE user_id = ?1)
                || '|' || (SELECT COUNT(*) || ':' || COALESCE(MAX(id), 0)
                             FROM chapter_read WHERE user_id = ?1)
                || '|' || (SELECT COUNT(*) || ':' || COALESCE(MAX(updated_at), '')
                             FROM season_rating WHERE user_id = ?1)
                || '|' || (SELECT COUNT(*) FROM shelf WHERE user_id = ?1)
                || '|' || (SELECT COUNT(*) FROM watch_with WHERE user_id = ?1)
                || '|' || (SELECT COUNT(*) FROM status_change WHERE user_id = ?1)
                || '|' || (SELECT COUNT(*) FROM badge_award WHERE user_id = ?1)
                || '|' || (SELECT COUNT(*) || ':' || COALESCE(MAX(updated_at), '')
                             FROM badge WHERE condition IS NOT NULL)",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(format!(
        "{today}|{}|{}|{counted}",
        rules::RULE_VERSION,
        catalogue::all().len()
    ))
}

/// Work one account's badges out, unless nothing changed since the last
/// time. Says what was given and taken.
pub async fn run(state: &AppState, user_id: i64, force: bool) -> AppResult<give::Settled> {
    let pool = &state.pool;
    let tz = crate::announce::time_zone(pool, user_id).await;
    let now = Utc::now();
    let today = eligible::day(now, tz);
    let mark = fingerprint(pool, user_id, &today.to_string()).await?;
    let seen: Option<(String,)> =
        sqlx::query_as("SELECT value FROM app_setting WHERE user_id = ? AND key = ?")
            .bind(user_id)
            .bind(SEEN_KEY)
            .fetch_optional(pool)
            .await?;
    let unchanged = seen.is_some_and(|(s,)| s == mark);
    if !force && unchanged && cached(user_id, &mark).is_some() {
        return Ok(give::Settled::default());
    }
    let facts = facts::read(pool, user_id, tz, now).await?;
    let ok = eligible::eligible(&facts.logged(), tz, now);
    let mut tally = rules::all(&facts, &ok);
    if !force && unchanged {
        // Nothing to give: only the progress was missing (a restart).
        let map = progress_of(pool, user_id, &tally).await?;
        keep(user_id, mark, map);
        return Ok(give::Settled::default());
    }
    let made_here: Vec<(crate::badges::Conditional, rules::Earned)> =
        crate::badges::conditional(pool)
            .await?
            .into_iter()
            .filter_map(|b| {
                let (_, earned) = custom::reach(&tally, b.id, &b.condition);
                earned.map(|e| (b, e))
            })
            .collect();
    let earned = std::mem::take(&mut tally.earned);
    let settled = give::settle(state, user_id, earned, today, now).await?;
    give::settle_custom(state, user_id, made_here, now).await?;
    // Awards change the count it was taken from; mark what is there now.
    let mark = fingerprint(pool, user_id, &today.to_string()).await?;
    let map = progress_of(pool, user_id, &tally).await?;
    keep(user_id, mark.clone(), map);
    sqlx::query(
        "INSERT INTO app_setting (user_id, key, value, updated_at) VALUES (?, ?, ?, ?)
         ON CONFLICT (user_id, key) DO UPDATE SET value = excluded.value,
                                                  updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(SEEN_KEY)
    .bind(&mark)
    .bind(now.stamp())
    .execute(pool)
    .await?;
    Ok(settled)
}

/// How far one account is along each badge counted in steps, as
/// (have, need) by catalogue key, for the list of badges still to come.
/// What the worker's last run worked out, while nothing has changed since;
/// otherwise worked out now, the way a run is, without giving anything.
pub async fn progress(pool: &SqlitePool, user_id: i64) -> AppResult<Progress> {
    let tz = crate::announce::time_zone(pool, user_id).await;
    let now = Utc::now();
    let mark = fingerprint(pool, user_id, &eligible::day(now, tz).to_string()).await?;
    if let Some(map) = cached(user_id, &mark) {
        return Ok(map);
    }
    let facts = facts::read(pool, user_id, tz, now).await?;
    let ok = eligible::eligible(&facts.logged(), tz, now);
    let tally = rules::all(&facts, &ok);
    let map = progress_of(pool, user_id, &tally).await?;
    keep(user_id, mark, map.clone());
    Ok(map)
}

async fn progress_of(pool: &SqlitePool, user_id: i64, tally: &rules::Tally) -> AppResult<Progress> {
    let mut progress = tally.progress.clone();
    for b in crate::badges::conditional(pool).await? {
        let (have, _) = custom::reach(tally, b.id, &b.condition);
        let need = b.condition.at as f64;
        progress.insert(custom::Condition::key(b.id), (have.min(need), need));
    }
    let (held,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM badge_award WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await?;
    for (key, need) in [("king-of-the-pond", 50.0), ("pond-legend", 100.0)] {
        progress.insert(key.to_string(), ((held as f64).min(need), need));
    }
    Ok(progress)
}

/// The worker's pass: every account that can sign in.
pub async fn tick(state: &AppState) -> AppResult<()> {
    let accounts: Vec<(i64,)> = sqlx::query_as("SELECT id FROM app_user WHERE disabled = 0")
        .fetch_all(&state.pool)
        .await?;
    for (id,) in accounts {
        if let Err(e) = run(state, id, false).await {
            tracing::warn!(account = id, error = %e, "badges could not be worked out");
        }
    }
    Ok(())
}
