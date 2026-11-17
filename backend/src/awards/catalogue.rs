//! Every automatic badge, from `backend/badges/catalogue.json`, and its art
//! from `backend/badges/art.json` (written by `npm run badges`). Both are
//! compiled in: a release carries the badges it knows how to give.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::OnceLock;

const CATALOGUE: &str = include_str!("../../badges/catalogue.json");
const ART: &str = include_str!("../../badges/art.json");

/// Whether a badge counts everything, or only watches recorded as they
/// happened (`super::eligible`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Total,
    Moment,
}

/// A badge's place on its ladder.
#[derive(Debug, Clone)]
pub struct Step {
    pub ladder: String,
    pub at: i64,
    /// 0 for the first step.
    pub index: usize,
    pub last: bool,
    /// Which metal it is drawn in: none for a first step at 1, then bronze
    /// (0), silver (1), gold (2) and on, as `build-badges.mjs` draws them.
    pub metal: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub key: String,
    pub name: String,
    pub about: String,
    pub kind: Kind,
    /// Which part of the badges page it sits in (`GROUPS`).
    pub group: String,
    /// How hard it is to earn, 1 (easy) to 5 (legendary), for sorting and
    /// the page's words. A ladder climbs from 1 at its first step to 5 at its
    /// last; a single badge goes by the ring it is drawn in.
    pub difficulty: u8,
    pub step: Option<Step>,
}

/// The parts of the badges page, in order. A badge given by hand sits in
/// `given`, which the catalogue never names.
pub const GROUPS: [&str; 7] = [
    "watching",
    "moments",
    "taste",
    "anime",
    "frog",
    "collector",
    "events",
];

#[derive(Deserialize)]
struct File {
    ladders: Vec<LadderFile>,
    singles: Vec<SingleFile>,
}

#[derive(Deserialize)]
struct LadderFile {
    id: String,
    kind: String,
    group: String,
    steps: Vec<StepFile>,
}

#[derive(Deserialize)]
struct StepFile {
    at: i64,
    key: String,
    name: String,
    about: String,
}

#[derive(Deserialize)]
struct SingleFile {
    key: String,
    kind: String,
    group: String,
    ring: String,
    name: String,
    about: String,
}

fn kind(word: &str) -> Kind {
    match word {
        "moment" => Kind::Moment,
        _ => Kind::Total,
    }
}

/// A ladder's step, from 1 at the bottom to 5 at the top.
fn climb(index: usize, count: usize) -> u8 {
    if count < 2 {
        return 1;
    }
    (1 + (4 * index + (count - 1) / 2) / (count - 1)) as u8
}

/// A single badge's ring, from the commonest to the rarest.
fn ring_difficulty(ring: &str) -> u8 {
    match ring {
        "lily" => 1,
        "sapphire" => 2,
        "amethyst" => 3,
        "aurora" => 4,
        _ => 2,
    }
}

/// Every automatic badge, ladders first, in the catalogue's order.
pub fn all() -> &'static [Entry] {
    static ALL: OnceLock<Vec<Entry>> = OnceLock::new();
    ALL.get_or_init(|| {
        let file: File = serde_json::from_str(CATALOGUE).expect("badges/catalogue.json");
        let mut out = Vec::new();
        for ladder in file.ladders {
            let count = ladder.steps.len();
            let mut metal = 0;
            for (index, s) in ladder.steps.into_iter().enumerate() {
                let m = if s.at == 1 {
                    None
                } else {
                    metal += 1;
                    Some(metal - 1)
                };
                out.push(Entry {
                    key: s.key,
                    name: s.name,
                    about: s.about,
                    kind: kind(&ladder.kind),
                    group: ladder.group.clone(),
                    difficulty: climb(index, count),
                    step: Some(Step {
                        ladder: ladder.id.clone(),
                        at: s.at,
                        index,
                        last: index + 1 == count,
                        metal: m,
                    }),
                });
            }
        }
        for s in file.singles {
            out.push(Entry {
                key: s.key,
                name: s.name,
                about: s.about,
                kind: kind(&s.kind),
                group: s.group,
                difficulty: ring_difficulty(&s.ring),
                step: None,
            });
        }
        // Each event's two, drawn with the event (`crate::events::pack`).
        for p in crate::events::pack::all() {
            if let Some(w) = &p.badges.ready {
                out.push(Entry {
                    key: crate::events::pack::ready_key(&p.slug),
                    name: w.name.clone(),
                    about: w.about.clone(),
                    kind: Kind::Total,
                    group: "events".into(),
                    difficulty: 3,
                    step: None,
                });
            }
            out.push(Entry {
                key: crate::events::pack::opening_key(&p.slug),
                name: p.badges.opening.name.clone(),
                about: p.badges.opening.about.clone(),
                kind: Kind::Moment,
                group: "events".into(),
                difficulty: 2,
                step: None,
            });
        }
        out
    })
}

pub fn get(key: &str) -> Option<&'static Entry> {
    all().iter().find(|e| e.key == key)
}

/// A ladder's steps, lowest first.
pub fn ladder(id: &str) -> impl Iterator<Item = &'static Entry> + '_ {
    all()
        .iter()
        .filter(move |e| e.step.as_ref().is_some_and(|s| s.ladder == id))
}

/// A badge's SVG.
pub fn art(key: &str) -> Option<&'static str> {
    static ART_MAP: OnceLock<HashMap<String, String>> = OnceLock::new();
    ART_MAP
        .get_or_init(|| serde_json::from_str(ART).expect("badges/art.json"))
        .get(key)
        .map(String::as_str)
        .or_else(|| crate::events::pack::art(key))
}

/// What a badge row was last brought up to: its words and its picture. A
/// new release that changes either changes this.
pub fn signature(entry: &Entry) -> String {
    let mut h = Sha256::new();
    h.update(entry.name.as_bytes());
    h.update([0]);
    h.update(entry.about.as_bytes());
    h.update([0]);
    h.update(art(&entry.key).unwrap_or_default().as_bytes());
    hex::encode(&h.finalize()[..12])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_badge_has_art_and_a_name_that_fits() {
        let mut keys = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        for e in all() {
            assert!(keys.insert(&e.key), "{} twice", e.key);
            assert!(
                names.insert(e.name.to_lowercase()),
                "the name {} twice",
                e.name
            );
            assert!(art(&e.key).is_some(), "no art for {}", e.key);
            assert!(
                GROUPS.contains(&e.group.as_str()),
                "{} in {}",
                e.key,
                e.group
            );
            assert!((1..=5).contains(&e.difficulty), "{}", e.key);
            assert!(crate::badges::check_name(&e.name).is_ok(), "{}", e.name);
            assert!(
                crate::badges::check_description(Some(&e.about)).is_ok(),
                "{}",
                e.key
            );
        }
    }

    #[test]
    fn a_ladder_climbs_from_easy_to_legendary() {
        let films: Vec<u8> = ladder("films").map(|e| e.difficulty).collect();
        assert_eq!(films.first(), Some(&1));
        assert_eq!(films.last(), Some(&5));
        assert!(films.windows(2).all(|w| w[0] <= w[1]), "{films:?}");
        assert_eq!(climb(0, 3), 1);
        assert_eq!(climb(1, 3), 3);
        assert_eq!(climb(2, 3), 5);
        assert_eq!(get("triple-bill").unwrap().difficulty, 2);
    }

    #[test]
    fn ladders_climb_and_take_their_metals_in_order() {
        let films: Vec<_> = ladder("films").collect();
        assert_eq!(films[0].step.as_ref().unwrap().metal, None);
        assert_eq!(films[1].step.as_ref().unwrap().metal, Some(0));
        assert!(films.last().unwrap().step.as_ref().unwrap().last);
        let library: Vec<_> = ladder("library").collect();
        assert_eq!(library[0].step.as_ref().unwrap().metal, Some(0));
        for e in all().iter().filter(|e| e.step.is_some()) {
            let s = e.step.as_ref().unwrap();
            let steps: Vec<_> = ladder(&s.ladder).collect();
            if s.index > 0 {
                assert!(
                    steps[s.index - 1].step.as_ref().unwrap().at < s.at,
                    "{}",
                    e.key
                );
            }
        }
    }
}
