//! The two badges every event gives (`crate::events`).
//!
//! Ready is a total: every essential seen, however it reached Frog, each
//! recorded before zero, given once zero has passed. Opening Night is a
//! moment: the event's own title (or an episode of its season) watched in
//! the days after zero, through `eligible`, on a day that is over, so
//! marking it in bulk later earns nothing. An event switched off gives
//! nothing; what was given stays.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

use super::facts::Facts;
use super::rules::{Earned, Evidence};
use crate::error::AppResult;
use crate::events::{self, pack, records, view};

/// What an account's badges depend on in events: which have reached zero
/// and which are past Opening Night, so a pinned moment that is not a
/// midnight still starts a run.
pub async fn mark(pool: &SqlitePool, tz: Tz, now: DateTime<Utc>) -> AppResult<String> {
    let mut out = String::new();
    for k in view::known(pool, false).await? {
        let z = k.zero_for(tz);
        out.push_str(&format!(
            "{}:{}:{};",
            k.row.slug,
            now >= z,
            now >= z + chrono::Duration::days(events::OPENING_DAYS)
        ));
    }
    Ok(out)
}

/// What one account has earned from events, and how far it is along each
/// Ready (have, need).
pub async fn earned(
    pool: &SqlitePool,
    user_id: i64,
    f: &Facts,
    eligible: &HashSet<i64>,
) -> AppResult<(Vec<Earned>, HashMap<String, (f64, f64)>)> {
    let mut out = Vec::new();
    let mut progress = HashMap::new();
    for k in view::known(pool, false).await? {
        let z = k.zero_for(f.tz);
        if k.pack.catch_up {
            let rows = records::titles(pool, k.row.id).await?;
            let stood = view::standings(pool, user_id, &rows).await?;
            let r = view::readiness(&stood, f.now, z);
            let key = pack::ready_key(&k.row.slug);
            progress.insert(
                key.clone(),
                (r.essentials_seen as f64, r.essentials.max(1) as f64),
            );
            if f.now >= z && r.ready_by_zero {
                out.push(Earned {
                    key,
                    on: f.day(z),
                    evidence: Evidence {
                        title: Some(k.title_id),
                        watches: Vec::new(),
                    },
                });
            }
        }
        let own = records::watches_of_target(pool, user_id, k.title_id, k.pack.season).await?;
        let first = f
            .watches
            .iter()
            .filter(|w| own.contains(&w.id) && eligible.contains(&w.id))
            .filter(|w| events::on_opening(w.watched_at, z))
            .filter(|w| f.day(w.watched_at) < f.today())
            .min_by_key(|w| (w.watched_at, w.id));
        if let Some(w) = first {
            out.push(Earned {
                key: pack::opening_key(&k.row.slug),
                on: f.day(w.watched_at),
                evidence: Evidence {
                    title: Some(k.title_id),
                    watches: vec![w.id],
                },
            });
        }
    }
    Ok((out, progress))
}
