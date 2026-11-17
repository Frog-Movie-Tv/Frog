//! An event as one account sees it: the hero on Home, the Before row, the
//! Events list and an event's own page. Everything here is read for the
//! account asking; other people appear only through `store::shown_to`.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::HashSet;

use super::pack::{self, Pack};
use super::records::{self, EventRow, ListRow};
use super::{per_day, phase, ready_by, zero, Phase, Standing};
use crate::catalog::{self, TitleRow};
use crate::clock::Stamp;
use crate::error::AppResult;
use crate::palette::Palette;
use crate::store::{self, ItemFacts, LibraryItem};

/// An event with its pack, as the database has it.
pub struct Known {
    pub row: EventRow,
    pub pack: &'static Pack,
    pub day: NaiveDate,
    pub title_id: i64,
}

/// Every event this release has a pack for, with its title made Frog's.
/// Off ones only for administrators.
pub async fn known(pool: &SqlitePool, with_off: bool) -> AppResult<Vec<Known>> {
    let mut out = Vec::new();
    for row in records::all(pool).await? {
        let Some(pack) = pack::get(&row.slug) else {
            continue;
        };
        let Some(title_id) = row.title_id else {
            continue;
        };
        if !with_off && !row.on() {
            continue;
        }
        let day = row.day().unwrap_or(pack.day);
        out.push(Known {
            row,
            pack,
            day,
            title_id,
        });
    }
    Ok(out)
}

pub async fn one(pool: &SqlitePool, slug: &str, with_off: bool) -> AppResult<Option<Known>> {
    Ok(known(pool, with_off)
        .await?
        .into_iter()
        .find(|k| k.row.slug == slug))
}

impl Known {
    pub fn zero_for(&self, tz: chrono_tz::Tz) -> DateTime<Utc> {
        zero(self.day, self.row.pinned(), tz)
    }
    pub fn phase_for(&self, tz: chrono_tz::Tz, now: DateTime<Utc>) -> Phase {
        phase(
            now,
            self.zero_for(tz),
            self.pack.shows_from_days,
            self.pack.ends_after_days,
        )
    }
}

/// How far one account is with the whole list.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Readiness {
    pub essentials_seen: i64,
    pub essentials: i64,
    pub optional_seen: i64,
    pub optional: i64,
    /// Minutes of essentials still to watch.
    pub minutes_left: i64,
    pub per_day: Option<i64>,
    /// The first essential not seen yet, in watch order.
    pub next: Option<Value>,
    pub ready: bool,
    /// Every essential seen, each recorded before zero.
    pub ready_by_zero: bool,
}

/// One title on the list, for this account.
#[derive(Debug, Clone, Serialize)]
pub struct ListItem {
    pub item: LibraryItem,
    pub group: String,
    pub essential: bool,
    pub note: Option<String>,
    pub seasons: Option<[i64; 2]>,
    pub standing: Standing,
}

/// The list's titles and how this account stands with each.
pub async fn standings(
    pool: &SqlitePool,
    user_id: i64,
    rows: &[ListRow],
) -> AppResult<Vec<(ListRow, TitleRow, Standing)>> {
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let Ok(title) = catalog::get_title(pool, r.title_id).await else {
            continue;
        };
        let s = records::standing(pool, user_id, r, &title.kind, title.runtime).await?;
        out.push((r.clone(), title, s));
    }
    Ok(out)
}

pub fn readiness(
    stood: &[(ListRow, TitleRow, Standing)],
    now: DateTime<Utc>,
    zero: DateTime<Utc>,
) -> Readiness {
    let essentials: Vec<_> = stood.iter().filter(|(r, _, _)| r.essential).collect();
    let optional: Vec<_> = stood.iter().filter(|(r, _, _)| !r.essential).collect();
    let minutes_left = essentials.iter().map(|(_, _, s)| s.minutes_left).sum();
    let next = essentials
        .iter()
        .find(|(_, _, s)| !s.seen)
        .map(|(_, t, s)| json!({ "title_id": t.id, "name": t.name, "code": s.next }));
    let essentials_seen = essentials.iter().filter(|(_, _, s)| s.seen).count() as i64;
    Readiness {
        essentials_seen,
        essentials: essentials.len() as i64,
        optional_seen: optional.iter().filter(|(_, _, s)| s.seen).count() as i64,
        optional: optional.len() as i64,
        minutes_left,
        per_day: per_day(minutes_left, now, zero),
        next,
        ready: !essentials.is_empty() && essentials_seen == essentials.len() as i64,
        ready_by_zero: ready_by(essentials.iter().map(|(_, _, s)| s), zero),
    }
}

fn accent(pack: &Pack, title: &TitleRow) -> Palette {
    let mut p = catalog::parse_accent(title.accent_json.as_deref());
    if let Some(a) = &pack.accent {
        p.accent = a.clone();
    }
    p
}

/// Who is looking, and when: their zone, and which events they hid or
/// already saw open.
pub struct Viewer {
    pub user_id: i64,
    pub tz: chrono_tz::Tz,
    pub now: DateTime<Utc>,
    hidden: HashSet<i64>,
    seen: HashSet<i64>,
}

impl Viewer {
    pub async fn load(pool: &SqlitePool, user_id: i64) -> AppResult<Self> {
        Ok(Self {
            user_id,
            tz: crate::announce::time_zone(pool, user_id).await,
            now: Utc::now(),
            hidden: records::hidden_ids(pool, user_id).await?,
            seen: records::seen_ids(pool, user_id).await?,
        })
    }
}

/// What the hero, the list and a list card share.
async fn summary(
    pool: &SqlitePool,
    v: &Viewer,
    k: &Known,
    facts: &ItemFacts,
) -> AppResult<(Value, Vec<ListItem>)> {
    let (user_id, tz, now) = (v.user_id, v.tz, v.now);
    let title = catalog::get_title(pool, k.title_id).await?;
    let z = k.zero_for(tz);
    let stood = if k.pack.catch_up {
        standings(pool, user_id, &records::titles(pool, k.row.id).await?).await?
    } else {
        Vec::new()
    };
    let ready = k.pack.catch_up.then(|| readiness(&stood, now, z));
    // The premiere lands the Ready badge for whoever made it, so it needs the
    // picture before the worker has given it.
    let ready_badge = match k.pack.catch_up {
        true => crate::badges::by_rule(pool, user_id, &pack::ready_key(&k.row.slug))
            .await?
            .map(|(id, name, ..)| json!({ "name": name, "picture": format!("/api/badges/{id}/small") })),
        false => None,
    };
    let list: Vec<ListItem> = stood
        .into_iter()
        .map(|(r, t, s)| ListItem {
            item: facts.item(t),
            group: r.group_name,
            essential: r.essential,
            note: r.note,
            seasons: r.season_from.zip(r.season_to).map(|(a, b)| [a, b]),
            standing: s,
        })
        .collect();
    let value = json!({
        "slug": k.row.slug,
        "name": k.pack.name,
        "opens": k.pack.opens,
        "season": k.pack.season,
        "title": facts.item(title.clone()),
        "day": k.day.format("%Y-%m-%d").to_string(),
        "zero": z.stamp(),
        "shows_from": (z - Duration::days(k.pack.shows_from_days)).stamp(),
        "ends_at": (z + Duration::days(k.pack.ends_after_days)).stamp(),
        "pinned": k.row.pinned_at.is_some(),
        "phase": k.phase_for(tz, now),
        "hidden": v.hidden.contains(&k.row.id),
        "seen": v.seen.contains(&k.row.id),
        "catch_up": k.pack.catch_up,
        "accent": accent(k.pack, &title),
        "readiness": ready,
        "ready_badge": ready_badge,
        "left": list.iter().filter(|l| !l.standing.seen).count(),
    });
    Ok((value, list))
}

/// The events Home draws: live or just out, switched on, not hidden.
pub async fn for_home(pool: &SqlitePool, user_id: i64, facts: &ItemFacts) -> AppResult<Vec<Value>> {
    let viewer = Viewer::load(pool, user_id).await?;
    let mut out = Vec::new();
    for k in known(pool, false).await? {
        if viewer.hidden.contains(&k.row.id)
            || !matches!(
                k.phase_for(viewer.tz, viewer.now),
                Phase::Live | Phase::Premiered
            )
        {
            continue;
        }
        let (mut v, list) = summary(pool, &viewer, &k, facts).await?;
        v["before"] = json!(list);
        out.push(v);
    }
    // The nearest first: two on one day share the hero.
    out.sort_by(|a, b| a["zero"].as_str().cmp(&b["zero"].as_str()));
    Ok(out)
}

/// The Events page: now, scheduled and past, hidden ones marked.
pub async fn list(pool: &SqlitePool, user_id: i64, admin: bool) -> AppResult<Value> {
    let viewer = Viewer::load(pool, user_id).await?;
    let facts = ItemFacts::load(pool, user_id).await?;
    let mut events = Vec::new();
    for k in known(pool, admin).await? {
        let (mut v, _) = summary(pool, &viewer, &k, &facts).await?;
        v["on"] = json!(k.row.on());
        events.push(v);
    }
    events.sort_by(|a, b| a["zero"].as_str().cmp(&b["zero"].as_str()));
    Ok(json!({ "events": events }))
}

/// One event's own page.
pub async fn page(pool: &SqlitePool, user_id: i64, k: &Known) -> AppResult<Value> {
    let viewer = Viewer::load(pool, user_id).await?;
    let (tz, now) = (viewer.tz, viewer.now);
    let facts = ItemFacts::load(pool, user_id).await?;
    let (mut v, list) = summary(pool, &viewer, k, &facts).await?;

    let mut groups: Vec<Value> = Vec::new();
    for l in list {
        let same = groups
            .last()
            .is_some_and(|g| g["name"].as_str() == Some(&l.group));
        if !same {
            groups.push(json!({ "name": l.group, "titles": [] }));
        }
        if let Some(titles) = groups.last_mut().and_then(|g| g["titles"].as_array_mut()) {
            titles.push(json!(l));
        }
    }
    v["groups"] = json!(groups);
    v["on"] = json!(k.row.on());

    // Everyone the viewer may see, and the viewer first.
    let rows = records::titles(pool, k.row.id).await?;
    let z = k.zero_for(tz);
    let mut people = Vec::new();
    let mut accounts = vec![store::myself(pool, user_id).await?];
    accounts.extend(store::shown_to(pool, user_id).await?);
    for a in accounts {
        let mut p = json!({ "who": a.shown, "me": a.id == user_id });
        if k.pack.catch_up {
            let r = readiness(&standings(pool, a.id, &rows).await?, now, z);
            p["essentials_seen"] = json!(r.essentials_seen);
            p["essentials"] = json!(r.essentials);
            p["ready"] = json!(r.ready);
        } else {
            let entry = store::get_entry(pool, a.id, k.title_id).await?;
            p["status"] = json!(entry.map(|e| e.status));
        }
        people.push(p);
    }
    v["people"] = json!(people);

    let mut badges = Vec::new();
    let mut keys = vec![pack::opening_key(&k.row.slug)];
    if k.pack.catch_up {
        keys.insert(0, pack::ready_key(&k.row.slug));
    }
    for key in keys {
        let row = crate::badges::by_rule(pool, user_id, &key).await?;
        if let Some((id, name, about, held)) = row {
            badges.push(json!({
                "key": key,
                "name": name,
                "about": about,
                "held": held,
                "picture": format!("/api/badges/{id}/small"),
            }));
        }
    }
    v["badges"] = json!(badges);
    Ok(v)
}
