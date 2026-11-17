//! The events this release knows, compiled in from `backend/events/{slug}/`.
//!
//! Each event is made by hand, with the `/event` skill: its catch-up list,
//! two badges with art of their own, and a premiere animation in the
//! frontend (`lib/premieres/{slug}.ts`). A new event is one more `pack!`
//! line below and the folder beside it; nothing is written for it in the app.

use chrono::NaiveDate;
use serde::Deserialize;
use std::sync::OnceLock;

/// One event as its pack says it.
#[derive(Debug, Clone, Deserialize)]
pub struct Pack {
    pub slug: String,
    /// What the event is called in a sentence: "Before Doomsday".
    pub name: String,
    pub target: Ref,
    /// A show's season, when the event is a season premiere.
    #[serde(default)]
    pub season: Option<i64>,
    /// Follow TMDB's day for `FROG_REGION`, so a move moves the countdown.
    #[serde(default = "yes")]
    pub follows_release: bool,
    /// The day it opens when TMDB has none.
    pub day: NaiveDate,
    /// Where it opens: cinemas, streaming or season.
    pub opens: String,
    #[serde(default = "shows_from")]
    pub shows_from_days: i64,
    #[serde(default = "ends_after")]
    pub ends_after_days: i64,
    /// Without one the event is the countdown and the premiere alone.
    #[serde(default = "yes")]
    pub catch_up: bool,
    /// A colour of its own, else the backdrop's.
    #[serde(default)]
    pub accent: Option<String>,
    pub badges: Badges,
    #[serde(default)]
    pub groups: Vec<Group>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Ref {
    pub kind: String,
    pub tmdb: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Badges {
    /// Only with a catch-up list.
    #[serde(default)]
    pub ready: Option<Words>,
    pub opening: Words,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Words {
    pub name: String,
    pub about: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Group {
    pub name: String,
    pub titles: Vec<Entry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    pub kind: String,
    pub tmdb: i64,
    /// For a show: the seasons that count, first and last.
    #[serde(default)]
    pub seasons: Option<[i64; 2]>,
    pub essential: bool,
    #[serde(default)]
    pub note: Option<String>,
}

fn yes() -> bool {
    true
}
fn shows_from() -> i64 {
    45
}
fn ends_after() -> i64 {
    14
}

/// A pack as compiled in: its words and its two badge drawings.
struct Raw {
    slug: &'static str,
    json: &'static str,
    ready: Option<&'static str>,
    opening: &'static str,
}

macro_rules! pack {
    ($slug:literal) => {
        Raw {
            slug: $slug,
            json: include_str!(concat!("../../events/", $slug, "/event.json")),
            ready: Some(include_str!(concat!("../../events/", $slug, "/ready.svg"))),
            opening: include_str!(concat!("../../events/", $slug, "/opening.svg")),
        }
    };
    ($slug:literal, countdown only) => {
        Raw {
            slug: $slug,
            json: include_str!(concat!("../../events/", $slug, "/event.json")),
            ready: None,
            opening: include_str!(concat!("../../events/", $slug, "/opening.svg")),
        }
    };
}

/// Every pack, oldest first. The `/event` skill adds to the end.
const RAW: &[Raw] = &[pack!("avengers-doomsday")];

pub fn all() -> &'static [Pack] {
    static ALL: OnceLock<Vec<Pack>> = OnceLock::new();
    ALL.get_or_init(|| {
        RAW.iter()
            .map(|r| {
                let p: Pack = serde_json::from_str(r.json)
                    .unwrap_or_else(|e| panic!("events/{}/event.json: {e}", r.slug));
                assert_eq!(p.slug, r.slug, "a pack's slug is its folder");
                p
            })
            .collect()
    })
}

pub fn get(slug: &str) -> Option<&'static Pack> {
    all().iter().find(|p| p.slug == slug)
}

/// The badge keys an event gives.
pub fn ready_key(slug: &str) -> String {
    format!("event.{slug}.ready")
}
pub fn opening_key(slug: &str) -> String {
    format!("event.{slug}.opening")
}

/// A pack badge's drawing, by its catalogue key.
pub fn art(key: &str) -> Option<&'static str> {
    let rest = key.strip_prefix("event.")?;
    let (slug, which) = rest.rsplit_once('.')?;
    let raw = RAW.iter().find(|r| r.slug == slug)?;
    match which {
        "ready" => raw.ready,
        "opening" => Some(raw.opening),
        _ => None,
    }
}

impl Pack {
    /// Every title on the list, in watch order, with its group.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &Entry)> {
        self.groups
            .iter()
            .flat_map(|g| g.titles.iter().map(move |t| (g.name.as_str(), t)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pack_reads_and_keeps_its_own_rules() {
        let mut slugs = std::collections::HashSet::new();
        for p in all() {
            assert!(slugs.insert(&p.slug), "{} twice", p.slug);
            assert!(
                p.slug
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{}: a slug is lower case words and dashes",
                p.slug
            );
            assert!(matches!(p.target.kind.as_str(), "movie" | "tv"));
            assert_eq!(p.season.is_some(), p.target.kind == "tv", "{}", p.slug);
            assert!(matches!(
                p.opens.as_str(),
                "cinemas" | "streaming" | "season"
            ));
            assert!(p.shows_from_days > 0 && p.ends_after_days > 0);
            assert_eq!(p.catch_up, p.badges.ready.is_some(), "{}", p.slug);
            assert_eq!(p.catch_up, !p.groups.is_empty(), "{}", p.slug);
            if p.catch_up {
                assert!(p.entries().any(|(_, e)| e.essential), "{}", p.slug);
            }
            let mut seen = std::collections::HashSet::new();
            for (_, e) in p.entries() {
                assert!(matches!(e.kind.as_str(), "movie" | "tv"));
                assert!(
                    seen.insert((&e.kind, e.tmdb)),
                    "{} lists {} twice",
                    p.slug,
                    e.tmdb
                );
                assert!(e.kind == "tv" || e.seasons.is_none());
                if let Some([a, b]) = e.seasons {
                    assert!(a >= 1 && a <= b);
                }
                assert!(e.note.as_ref().is_none_or(|n| n.chars().count() <= 40));
                assert_ne!(
                    Ref {
                        kind: e.kind.clone(),
                        tmdb: e.tmdb
                    },
                    p.target
                );
            }
            for w in p.badges.ready.iter().chain([&p.badges.opening]) {
                assert!(crate::badges::check_name(&w.name).is_ok(), "{}", w.name);
                assert!(crate::badges::check_description(Some(&w.about)).is_ok());
            }
            assert!(art(&opening_key(&p.slug)).is_some());
            assert_eq!(art(&ready_key(&p.slug)).is_some(), p.catch_up);
        }
    }
}
