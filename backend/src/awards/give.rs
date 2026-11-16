//! Giving automatic badges, keeping them, and taking them back in their first
//! day.
//!
//! Each catalogue badge is one `badge` row with its key in `rule`. Awards
//! made here have no `awarded_by`; one given by hand never comes back through
//! this module. For `GRACE_HOURS` after an award, an account whose watches no
//! longer earn it (removed, undone, a date changed, AniList took it back)
//! loses it again, and its notification goes when nothing it announced is
//! left. After that it stays for good, whatever happens to the library.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde_json::json;
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

use super::catalogue::{self, Entry};
use super::eligible::parse;
use super::rules::{self, Earned, RULE_VERSION};
use crate::error::AppResult;
use crate::state::AppState;

pub const GRACE_HOURS: i64 = 24;

/// Bring the `badge` table up to the catalogue: add what is missing, and give
/// a badge whose words or picture changed in this release its new ones
/// (its words only while no administrator has renamed it). A name a
/// hand-made badge already has is never taken from it: that automatic badge
/// waits (`waiting`) until an administrator renames one of the two.
pub async fn seed(pool: &SqlitePool) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    for e in catalogue::all() {
        let sig = catalogue::signature(e);
        let row: Option<(i64, Option<String>, bool)> =
            sqlx::query_as("SELECT id, rule_sig, rule_kept FROM badge WHERE rule = ?")
                .bind(&e.key)
                .fetch_optional(pool)
                .await?;
        match row {
            Some((_, Some(had), _)) if had == sig => {}
            Some((id, _, kept)) => {
                let words = !kept && !name_taken(pool, &e.name, Some(id)).await?;
                sqlx::query(
                    "UPDATE badge
                        SET name = CASE WHEN ?1 THEN ?2 ELSE name END,
                            description = CASE WHEN ?1 THEN ?3 ELSE description END,
                            rule_sig = ?4, updated_at = ?5
                      WHERE id = ?6",
                )
                .bind(words)
                .bind(&e.name)
                .bind(&e.about)
                .bind(&sig)
                .bind(&now)
                .bind(id)
                .execute(pool)
                .await?;
            }
            None if name_taken(pool, &e.name, None).await? => {
                tracing::warn!(badge = %e.name, "an automatic badge waits: a badge already has its name");
            }
            None => {
                sqlx::query(
                    "INSERT INTO badge (name, description, large, small, original, original_mime,
                                        settings_json, created_at, updated_at, rule, rule_sig)
                     VALUES (?, ?, x'', x'', x'', 'image/png', '{}', ?, ?, ?, ?)",
                )
                .bind(&e.name)
                .bind(&e.about)
                .bind(&now)
                .bind(&now)
                .bind(&e.key)
                .bind(&sig)
                .execute(pool)
                .await?;
            }
        }
    }
    Ok(())
}

async fn name_taken(pool: &SqlitePool, name: &str, other_than: Option<i64>) -> AppResult<bool> {
    let (taken,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM badge WHERE name = ? COLLATE NOCASE AND id IS NOT ?)",
    )
    .bind(name)
    .bind(other_than)
    .fetch_one(pool)
    .await?;
    Ok(taken)
}

/// Catalogue badges with no row, because a badge already has their name.
pub async fn waiting(pool: &SqlitePool) -> AppResult<Vec<&'static Entry>> {
    let have: Vec<(String,)> = sqlx::query_as("SELECT rule FROM badge WHERE rule IS NOT NULL")
        .fetch_all(pool)
        .await?;
    let have: HashSet<String> = have.into_iter().map(|(r,)| r).collect();
    Ok(catalogue::all()
        .iter()
        .filter(|e| !have.contains(&e.key))
        .collect())
}

/// (badge id, rule, awarded_at, awarded_by, evidence)
type AwardRow = (i64, Option<String>, String, Option<i64>, Option<String>);

/// What one run did for one account.
#[derive(Debug, Default)]
pub struct Settled {
    pub given: Vec<String>,
    pub taken: Vec<String>,
}

/// Make one account's automatic badges match `earned`: give what is new,
/// take back what is no longer earned while it is in its grace day, and tell
/// the account once about everything given.
pub async fn settle(
    state: &AppState,
    user_id: i64,
    earned: Vec<Earned>,
    today: NaiveDate,
    now: DateTime<Utc>,
) -> AppResult<Settled> {
    let pool = &state.pool;
    let held: Vec<AwardRow> = sqlx::query_as(
        "SELECT a.badge_id, b.rule, a.awarded_at, a.awarded_by, a.evidence
           FROM badge_award a JOIN badge b ON b.id = a.badge_id
          WHERE a.user_id = ?",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let ids: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, rule FROM badge WHERE rule IS NOT NULL")
            .fetch_all(pool)
            .await?;
    let id_of: HashMap<String, i64> = ids.into_iter().map(|(id, r)| (r, id)).collect();

    let automatic = |r: &AwardRow| r.1.as_deref().is_some_and(|k| catalogue::get(k).is_some());
    let hand_made = held.iter().filter(|r| !automatic(r)).count();
    let in_grace = |r: &AwardRow| {
        r.3.is_none() && parse(&r.2).is_some_and(|at| now - at < Duration::hours(GRACE_HOURS))
    };

    // Badges about badges count what is held once this run is done.
    let mut keys: HashSet<String> = earned.iter().map(|e| e.key.clone()).collect();
    let mut after: HashSet<String> = held
        .iter()
        .filter(|r| automatic(r))
        .filter(|r| !in_grace(r) || keys.contains(r.1.as_deref().unwrap_or_default()))
        .filter_map(|r| r.1.clone())
        .collect();
    after.extend(keys.iter().cloned());
    let mut earned = earned;
    for m in rules::meta(&after, hand_made, today) {
        if keys.insert(m.key.clone()) {
            earned.push(m);
        }
    }

    let mut out = Settled::default();
    let mut runs = HashSet::new();
    for r in held.iter().filter(|r| automatic(r) && in_grace(r)) {
        let key = r.1.clone().unwrap_or_default();
        if keys.contains(&key) {
            continue;
        }
        sqlx::query("DELETE FROM badge_award WHERE user_id = ? AND badge_id = ?")
            .bind(user_id)
            .bind(r.0)
            .execute(pool)
            .await?;
        if let Some(run) =
            r.4.as_deref()
                .and_then(|e| serde_json::from_str::<serde_json::Value>(e).ok())
                .and_then(|v| v["run"].as_i64())
        {
            runs.insert(run);
        }
        out.taken.push(key);
    }
    for run in runs {
        let (left,): (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM badge_award
                             WHERE user_id = ? AND json_extract(evidence, '$.run') = ?)",
        )
        .bind(user_id)
        .bind(run)
        .fetch_one(pool)
        .await?;
        if !left {
            crate::inbox::forget(pool, user_id, &ref_key(user_id, run)).await?;
        }
    }

    let had: HashSet<i64> = held.iter().map(|r| r.0).collect();
    let run = now.timestamp_millis();
    let stamp = now.to_rfc3339();
    let mut given: Vec<&Earned> = earned
        .iter()
        .filter(|e| id_of.get(&e.key).is_some_and(|id| !had.contains(id)))
        .collect();
    // Oldest first, so the holder's order reads as they earned them.
    given.sort_by_key(|e| (e.on, catalogue::all().iter().position(|c| c.key == e.key)));
    for e in &given {
        let mut evidence = serde_json::to_value(&e.evidence).unwrap_or_else(|_| json!({}));
        evidence["run"] = json!(run);
        sqlx::query(
            "INSERT INTO badge_award (user_id, badge_id, awarded_at, awarded_by, shown, position,
                                      earned_on, evidence, rule_version)
             VALUES (?1, ?2, ?3, NULL, 1,
                     COALESCE((SELECT MAX(position) FROM badge_award WHERE user_id = ?1), -1) + 1,
                     ?4, ?5, ?6)
             ON CONFLICT (user_id, badge_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(id_of[&e.key])
        .bind(&stamp)
        .bind(e.on.to_string())
        .bind(evidence.to_string())
        .bind(RULE_VERSION)
        .execute(pool)
        .await?;
        out.given.push(e.key.clone());
    }
    if !given.is_empty() {
        tell(state, user_id, run, &given).await?;
    }
    Ok(out)
}

pub fn ref_key(user_id: i64, run: i64) -> String {
    format!("badges:{user_id}:{run}")
}

/// One notification for a run, however many it gave.
async fn tell(state: &AppState, user_id: i64, run: i64, given: &[&Earned]) -> AppResult<()> {
    let names: Vec<&str> = given
        .iter()
        .filter_map(|e| catalogue::get(&e.key).map(|c| c.name.as_str()))
        .collect();
    let username = crate::badges::holder_name(&state.pool, user_id)
        .await?
        .unwrap_or_default();
    let (title, body) = match names.as_slice() {
        [one] => (
            format!("You earned {one}"),
            catalogue::get(&given[0].key)
                .map(|c| c.about.clone())
                .unwrap_or_default(),
        ),
        many => (format!("You earned {} badges", many.len()), many.join(", ")),
    };
    let push = json!({
        "title": title,
        "body": body,
        "url": format!("/people/{}/badges", urlencoding::encode(&username)),
        "tag": format!("frog-badges-{run}"),
    });
    crate::inbox::tell(
        state,
        user_id,
        &crate::inbox::Event {
            kind: crate::inbox::BADGE,
            actor_id: None,
            payload: json!({
                "earned": true,
                "count": names.len(),
                "name": names.first(),
                "names": names,
                "username": username,
            }),
            ref_key: Some(ref_key(user_id, run)),
            ..Default::default()
        },
        Some(push),
    )
    .await?;
    Ok(())
}
