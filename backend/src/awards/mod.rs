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
//! - `art`: pictures drawn from the SVGs, never stored.

pub mod art;
pub mod catalogue;
pub mod eligible;
pub mod facts;
pub mod give;
pub mod rules;

use chrono::Utc;
use sqlx::SqlitePool;
use std::collections::HashMap;

use crate::error::AppResult;
use crate::state::AppState;

/// Where an account's last run is marked: what it counted, and the day.
const SEEN_KEY: &str = "badges_seen";

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
                || '|' || (SELECT COUNT(*) FROM badge_award WHERE user_id = ?1)",
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
    if !force && seen.is_some_and(|(s,)| s == mark) {
        return Ok(give::Settled::default());
    }
    let facts = facts::read(pool, user_id, tz, now).await?;
    let ok = eligible::eligible(&facts.logged(), tz, now);
    let earned = rules::all(&facts, &ok).earned;
    let settled = give::settle(state, user_id, earned, today, now).await?;
    // Awards change the count it was taken from; mark what is there now.
    let mark = fingerprint(pool, user_id, &today.to_string()).await?;
    sqlx::query(
        "INSERT INTO app_setting (user_id, key, value, updated_at) VALUES (?, ?, ?, ?)
         ON CONFLICT (user_id, key) DO UPDATE SET value = excluded.value,
                                                  updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(SEEN_KEY)
    .bind(&mark)
    .bind(now.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(settled)
}

/// How far one account is along each badge counted in steps, as
/// (have, need) by catalogue key, for the list of badges still to come.
/// Worked out the way a run is, without giving anything.
pub async fn progress(pool: &SqlitePool, user_id: i64) -> AppResult<HashMap<String, (f64, f64)>> {
    let tz = crate::announce::time_zone(pool, user_id).await;
    let now = Utc::now();
    let facts = facts::read(pool, user_id, tz, now).await?;
    let ok = eligible::eligible(&facts.logged(), tz, now);
    let mut progress = rules::all(&facts, &ok).progress;
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
