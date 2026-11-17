//! What each automatic badge asks, on plain facts (`super::facts`).
//!
//! `earned` answers every badge the account has earned right now, with the
//! day it was earned and what it rests on. Giving, keeping and taking back are
//! `super::give`'s. Totals read every watch of a title in the library, as Stats
//! does; moments read only eligible watches (`super::eligible`) on days that
//! are over, since rule 8 needs the whole day.

use chrono::{DateTime, Datelike, Duration, NaiveDate, Timelike, Utc, Weekday};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

use super::catalogue;
use super::facts::{Entry, Episode, Facts, Watch};

/// Raised when a rule changes what it means, so an award says which rule
/// gave it.
pub const RULE_VERSION: i64 = 1;

/// What an award rests on, kept with it as JSON.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Evidence {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<i64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub watches: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Earned {
    pub key: String,
    pub on: NaiveDate,
    pub evidence: Evidence,
}

/// Everything earned, and how far along each badge counted in steps is
/// (have, need), for the page that lists the ones still to come.
#[derive(Debug, Default)]
pub struct Tally {
    pub earned: Vec<Earned>,
    pub progress: HashMap<String, (f64, f64)>,
    /// Each ladder's running count, item by item in the order they
    /// happened, for badges made here that ask for a count of their own
    /// (`super::custom`).
    pub reached: HashMap<String, Vec<Reached>>,
}

/// A ladder's count after one more thing, the day it happened and what it was.
#[derive(Debug, Clone, PartialEq)]
pub struct Reached {
    pub sum: f64,
    pub on: NaiveDate,
    pub title: Option<i64>,
    pub watch: Option<i64>,
}

impl Tally {
    /// One award a badge: the earliest, should a rule meet it twice.
    fn push(&mut self, e: Earned) {
        match self.earned.iter_mut().find(|had| had.key == e.key) {
            Some(had) if e.on < had.on => *had = e,
            Some(_) => {}
            None => self.earned.push(e),
        }
    }

    fn count(&mut self, key: &str, have: f64, need: f64) {
        self.progress
            .insert(key.to_string(), (have.min(need), need));
    }
}

/// One thing counted toward a ladder, on the day it happened.
#[derive(Debug, Clone)]
struct Item {
    on: NaiveDate,
    title: Option<i64>,
    watch: Option<i64>,
    weight: f64,
}

impl Item {
    fn of(f: &Facts, w: &Watch) -> Self {
        Self {
            on: f.day(w.watched_at),
            title: Some(w.title_id),
            watch: Some(w.id),
            weight: 1.0,
        }
    }

    fn on(on: NaiveDate, title: Option<i64>) -> Self {
        Self {
            on,
            title,
            watch: None,
            weight: 1.0,
        }
    }
}

fn earned(key: &str, on: NaiveDate, title: Option<i64>, watches: Vec<i64>) -> Earned {
    Earned {
        key: key.to_string(),
        on,
        evidence: Evidence { title, watches },
    }
}

/// Every step of a ladder the items reach, in the order they happened: a
/// step is earned on the day of the item that reached it.
fn climb(out: &mut Tally, ladder: &str, mut items: Vec<Item>) {
    items.sort_by_key(|i| (i.on, i.watch));
    let mut sum = 0.0;
    let mut reached = Vec::with_capacity(items.len());
    for i in &items {
        sum += i.weight;
        reached.push(sum);
    }
    out.reached.insert(
        ladder.to_string(),
        items
            .iter()
            .zip(&reached)
            .map(|(i, sum)| Reached {
                sum: *sum,
                on: i.on,
                title: i.title,
                watch: i.watch,
            })
            .collect(),
    );
    for step in catalogue::ladder(ladder) {
        let at = step.step.as_ref().map_or(0, |s| s.at) as f64;
        out.count(&step.key, sum, at);
        if let Some(n) = reached.iter().position(|s| *s >= at) {
            let i = &items[n];
            out.push(earned(
                &step.key,
                i.on,
                i.title,
                i.watch.into_iter().collect(),
            ));
        }
    }
}

/// A single badge, earned on the day of the item that met it.
fn once(out: &mut Tally, key: &str, item: Option<Item>) {
    if let Some(i) = item {
        out.push(earned(key, i.on, i.title, i.watch.into_iter().collect()));
    }
}

/// The `n`th thing counted, in the order they happened.
fn nth(mut items: Vec<Item>, n: usize) -> Option<Item> {
    if n == 0 || items.len() < n {
        return None;
    }
    items.sort_by_key(|i| (i.on, i.watch));
    items.into_iter().nth(n - 1)
}

/// The first watch of each title or episode, so a rewatch adds nothing.
fn firsts<'a>(
    watches: impl Iterator<Item = &'a Watch>,
    by: impl Fn(&Watch) -> i64,
) -> Vec<&'a Watch> {
    let mut first: HashMap<i64, &Watch> = HashMap::new();
    for w in watches {
        let k = by(w);
        match first.get(&k) {
            Some(had) if (had.watched_at, had.id) <= (w.watched_at, w.id) => {}
            _ => {
                first.insert(k, w);
            }
        }
    }
    first.into_values().collect()
}

pub fn all(f: &Facts, eligible: &HashSet<i64>) -> Tally {
    let mut out = totals(f);
    for more in [moments(f, eligible), status_history(f, eligible)] {
        for e in more.earned {
            out.push(e);
        }
        out.progress.extend(more.progress);
        out.reached.extend(more.reached);
    }
    out
}

// --- status history ------------------------------------------------------------

/// How near a status change a watch recorded as it happened has to be for
/// that change to be watching, not tidying the library.
const CHANGE_HOURS: i64 = 36;
/// How long after a dropped show is picked up again its next watch may come.
const PICKED_UP_DAYS: i64 = 14;
/// The watchlist Watchlist Zero asks to be emptied.
const WATCHLIST_ZERO_FROM: i64 = 20;

/// Back From the Dead and Watchlist Zero, from status changes, which Frog
/// keeps since 1.9.1, so neither looks further back. Each needs watches
/// recorded as they happened, so edit mode, an import or a sync moving
/// titles about earns neither.
pub fn status_history(f: &Facts, eligible: &HashSet<i64>) -> Tally {
    let mut out = Tally::default();
    let ws = finished_days(f, eligible);
    let near = Duration::hours(CHANGE_HOURS);
    let watched = |title: i64, from: DateTime<Utc>, to: DateTime<Utc>| {
        ws.iter()
            .copied()
            .find(|w| w.title_id == title && w.watched_at >= from && w.watched_at <= to)
    };
    let kind = |title: i64| {
        f.entries
            .iter()
            .find(|e| e.title_id == title)
            .map(|e| e.kind.as_str())
    };
    let onward = |to: Option<&str>| matches!(to, Some("watching" | "completed"));

    // A dropped show taken up again, and an episode of it watched.
    let back = f
        .changes
        .iter()
        .filter(|c| c.from.as_deref() == Some("dropped") && onward(c.to.as_deref()))
        .filter(|c| kind(c.title_id) == Some("tv"))
        .filter_map(|c| {
            watched(
                c.title_id,
                c.at - near,
                c.at + Duration::days(PICKED_UP_DAYS),
            )
        })
        .min_by_key(|w| (w.watched_at, w.id));
    if let Some(w) = back {
        out.push(earned(
            "back-from-the-dead",
            f.day(w.watched_at),
            Some(w.title_id),
            vec![w.id],
        ));
    }

    // The watchlist, worked back from today's to before the first change,
    // then forward: from 20 or more to none, most of it by watching, the
    // last one too.
    let planned = |s: &Option<String>| s.as_deref() == Some("planned");
    let now = f.entries.iter().filter(|e| e.status == "planned").count() as i64;
    let ins = f.changes.iter().filter(|c| planned(&c.to)).count() as i64;
    let outs = f.changes.iter().filter(|c| planned(&c.from)).count() as i64;
    let mut held = now - ins + outs;
    let mut peak = held;
    let (mut left, mut by_watching) = (0, Vec::<&Watch>::new());
    for c in &f.changes {
        if planned(&c.to) {
            held += 1;
            peak = peak.max(held);
        }
        if !planned(&c.from) {
            continue;
        }
        held -= 1;
        left += 1;
        let w = onward(c.to.as_deref())
            .then(|| watched(c.title_id, c.at - near, c.at + near))
            .flatten();
        if let Some(w) = w {
            by_watching.push(w);
        }
        if held > 0 {
            continue;
        }
        if let Some(last) =
            w.filter(|_| peak >= WATCHLIST_ZERO_FROM && by_watching.len() as i64 * 2 > left)
        {
            out.push(earned(
                "watchlist-zero",
                f.day(last.watched_at),
                Some(last.title_id),
                by_watching.iter().map(|w| w.id).collect(),
            ));
        }
        (held, peak, left) = (0, 0, 0);
        by_watching.clear();
    }
    out
}

// --- totals --------------------------------------------------------------------

pub fn totals(f: &Facts) -> Tally {
    let mut out = Tally::default();
    let today = f.today();
    let lib: HashMap<i64, &Entry> = f.entries.iter().map(|e| (e.title_id, e)).collect();
    // A watch counts once it has happened, and only for a title still kept.
    let watches: Vec<&Watch> = f
        .watches
        .iter()
        .filter(|w| lib.contains_key(&w.title_id) && w.watched_at <= f.now)
        .collect();
    let kind = |w: &Watch| lib.get(&w.title_id).map_or("", |e| e.kind.as_str());
    let films: Vec<&Watch> = watches
        .iter()
        .copied()
        .filter(|w| w.episode_id.is_none() && kind(w) == "movie")
        .collect();
    let episodes: Vec<&Watch> = watches
        .iter()
        .copied()
        .filter(|w| w.episode_id.is_some())
        .collect();
    let watched_titles: HashSet<i64> = watches.iter().map(|w| w.title_id).collect();

    let first_films = firsts(films.iter().copied(), |w| w.title_id);
    climb(
        &mut out,
        "films",
        first_films.iter().map(|w| Item::of(f, w)).collect(),
    );
    let first_episodes = firsts(episodes.iter().copied(), |w| {
        w.episode_id.unwrap_or_default()
    });
    climb(
        &mut out,
        "episodes",
        first_episodes.iter().map(|w| Item::of(f, w)).collect(),
    );
    climb(
        &mut out,
        "hours",
        watches
            .iter()
            .map(|w| Item {
                weight: w.minutes / 60.0,
                ..Item::of(f, w)
            })
            .collect(),
    );
    climb(
        &mut out,
        "library",
        f.entries
            .iter()
            .map(|e| Item::on(e.added_at.map_or(today, |t| f.day(t)), Some(e.title_id)))
            .collect(),
    );

    // Finished shows, on the day they were finished where Frog knows it.
    let finished = |e: &&Entry| e.status == "completed";
    let finished_on = |e: &Entry| {
        e.finished_at.map(|t| f.day(t)).or_else(|| {
            watches
                .iter()
                .filter(|w| w.title_id == e.title_id && !w.undated)
                .map(|w| f.day(w.watched_at))
                .max()
        })
    };
    let shows_done: Vec<&Entry> = f
        .entries
        .iter()
        .filter(|e| e.kind == "tv")
        .filter(finished)
        .collect();
    climb(
        &mut out,
        "completed",
        shows_done
            .iter()
            .map(|e| Item::on(finished_on(e).unwrap_or(today), Some(e.title_id)))
            .collect(),
    );

    // Ratings and favourites keep no day of their own: earned the day it is seen.
    let rated: Vec<&Entry> = f.entries.iter().filter(|e| e.rating.is_some()).collect();
    climb(
        &mut out,
        "ratings",
        rated
            .iter()
            .map(|e| Item::on(today, Some(e.title_id)))
            .collect(),
    );
    climb(
        &mut out,
        "favourites",
        f.entries
            .iter()
            .filter(|e| e.favourite)
            .map(|e| Item::on(today, Some(e.title_id)))
            .collect(),
    );
    climb(
        &mut out,
        "notes",
        f.entries
            .iter()
            .filter(|e| e.has_notes)
            .map(|e| Item::on(today, Some(e.title_id)))
            .collect(),
    );

    // Chapters: how far each manga is read, on the days Frog has for them.
    let read: i64 = f
        .entries
        .iter()
        .filter(|e| e.kind == "manga")
        .filter_map(|e| e.progress)
        .map(|p| p.max(0))
        .sum();
    let mut chapters: Vec<Item> = f.reads.iter().map(|d| Item::on(*d, None)).collect();
    let top = catalogue::ladder("chapters")
        .filter_map(|e| e.step.as_ref())
        .map(|s| s.at)
        .max()
        .unwrap_or(0);
    while (chapters.len() as i64) < read.min(top) {
        chapters.push(Item::on(today, None));
    }
    climb(&mut out, "chapters", chapters);
    climb(
        &mut out,
        "manga",
        f.entries
            .iter()
            .filter(|e| e.kind == "manga" && e.status == "completed")
            .map(|e| Item::on(e.finished_at.map_or(today, |t| f.day(t)), Some(e.title_id)))
            .collect(),
    );

    // Rewatches: a film's are its watches, a show's are counted on its entry.
    let mut again: Vec<Item> = films
        .iter()
        .filter(|w| w.is_rewatch)
        .map(|w| Item::of(f, w))
        .collect();
    for e in f
        .entries
        .iter()
        .filter(|e| e.kind == "tv" && e.rewatch_count > 0)
    {
        for _ in 0..e.rewatch_count.min(100) {
            again.push(Item::on(
                e.finished_at.map_or(today, |t| f.day(t)),
                Some(e.title_id),
            ));
        }
    }
    climb(&mut out, "rewatches", again);
    climb(
        &mut out,
        "extension",
        watches
            .iter()
            .filter(|w| w.source == "extension")
            .map(|w| Item::of(f, w))
            .collect(),
    );
    climb(
        &mut out,
        "together",
        watches
            .iter()
            .filter(|w| w.with_someone)
            .map(|w| Item::of(f, w))
            .collect(),
    );
    if let Some(joined) = f.joined {
        let start = f.day(joined);
        let years: Vec<Item> = (1..=10)
            .filter_map(|n| {
                start
                    .with_year(start.year() + n)
                    .or_else(|| start.checked_add_signed(Duration::days(365 * n as i64)))
            })
            .take_while(|d| *d <= today)
            .map(|d| Item::on(d, None))
            .collect();
        climb(&mut out, "years", years);
    }

    let anime_first = firsts(
        watches
            .iter()
            .copied()
            .filter(|w| lib.get(&w.title_id).is_some_and(|e| e.anime)),
        |w| w.title_id,
    );
    climb(
        &mut out,
        "anime",
        anime_first.iter().map(|w| Item::of(f, w)).collect(),
    );
    let anime_films: Vec<Item> = first_films
        .iter()
        .filter(|w| lib.get(&w.title_id).is_some_and(|e| e.anime))
        .map(|w| Item::of(f, w))
        .collect();
    out.count("anime-film-club", anime_films.len() as f64, 10.0);
    once(&mut out, "anime-film-club", nth(anime_films, 10));

    // Seasons finished: every released episode of a season watched.
    let seen: HashMap<i64, &Watch> = first_episodes
        .iter()
        .map(|w| (w.episode_id.unwrap_or_default(), *w))
        .collect();
    let mut seasons: BTreeMap<(i64, i64), Vec<&Episode>> = BTreeMap::new();
    for e in f.episodes.iter().filter(|e| e.released) {
        seasons.entry((e.title_id, e.season)).or_default().push(e);
    }
    let mut seasons_done = Vec::new();
    for ((title, _), eps) in &seasons {
        if !lib.contains_key(title) {
            continue;
        }
        let last = eps
            .iter()
            .map(|e| seen.get(&e.id))
            .collect::<Option<Vec<_>>>();
        if let Some(ws) = last {
            if let Some(w) = ws.iter().max_by_key(|w| (w.watched_at, w.id)) {
                seasons_done.push(Item::of(f, w));
            }
        }
    }
    climb(&mut out, "seasons", seasons_done);

    // Days with something watched on them.
    let mut days: BTreeMap<NaiveDate, &Watch> = BTreeMap::new();
    for w in watches.iter().filter(|w| !w.undated) {
        days.entry(f.day(w.watched_at)).or_insert(w);
    }
    climb(
        &mut out,
        "days",
        days.values().map(|w| Item::of(f, w)).collect(),
    );

    // Long shows, finished.
    let regular: HashMap<i64, i64> = seasons.iter().fold(HashMap::new(), |mut m, ((t, _), eps)| {
        *m.entry(*t).or_default() += eps.len() as i64;
        m
    });
    for (key, least) in [("long-haul", 200), ("grand-voyage", 1000)] {
        once(
            &mut out,
            key,
            shows_done
                .iter()
                .filter(|e| {
                    regular
                        .get(&e.title_id)
                        .copied()
                        .or(e.episode_count)
                        .unwrap_or(0)
                        >= least
                })
                .map(|e| Item::on(finished_on(e).unwrap_or(today), Some(e.title_id)))
                .min_by_key(|i| i.on),
        );
    }

    // Up to date on running shows, now.
    let running: Vec<i64> = f
        .entries
        .iter()
        .filter(|e| e.kind == "tv" && e.status == "watching")
        .filter(|e| {
            matches!(
                e.tmdb_status.as_deref(),
                Some("Returning Series" | "In Production")
            )
        })
        .filter(|e| {
            let eps: Vec<&&Episode> = seasons
                .iter()
                .filter(|((t, _), _)| *t == e.title_id)
                .flat_map(|(_, v)| v.iter())
                .collect();
            !eps.is_empty() && eps.iter().all(|ep| seen.contains_key(&ep.id))
        })
        .map(|e| e.title_id)
        .collect();
    if running.len() >= 5 {
        once(
            &mut out,
            "caught-up",
            Some(Item::on(today, running.first().copied())),
        );
    }

    // Ratings.
    let perfect: Vec<Item> = rated
        .iter()
        .filter(|e| e.rating >= Some(100.0))
        .map(|e| Item::on(today, Some(e.title_id)))
        .collect();
    once(&mut out, "perfect-ten", nth(perfect, 10));
    let scores: Vec<f64> = rated.iter().filter_map(|e| e.rating).collect();
    if scores.len() >= 50 {
        let average = scores.iter().sum::<f64>() / scores.len() as f64;
        if average < 60.0 {
            once(&mut out, "tough-crowd", Some(Item::on(today, None)));
        }
        if average > 85.0 {
            once(&mut out, "easy-to-please", Some(Item::on(today, None)));
        }
    }
    let seen_titles: Vec<&Entry> = f
        .entries
        .iter()
        .filter(|e| {
            e.kind != "manga" && (watched_titles.contains(&e.title_id) || e.status == "completed")
        })
        .collect();
    if seen_titles.len() >= 25 && seen_titles.iter().all(|e| e.rating.is_some()) {
        once(&mut out, "deck-clearer", Some(Item::on(today, None)));
    }
    let tmdb = |e: &Entry| e.tmdb_vote.filter(|v| *v > 0.0).map(|v| v * 10.0);
    once(
        &mut out,
        "hot-take",
        rated
            .iter()
            .find(|e| matches!((e.rating, tmdb(e)), (Some(r), Some(t)) if r <= t - 30.0))
            .map(|e| Item::on(today, Some(e.title_id))),
    );
    once(
        &mut out,
        "hidden-gem",
        rated
            .iter()
            .find(|e| matches!((e.rating, tmdb(e)), (Some(r), Some(t)) if r >= 80.0 && t < 60.0))
            .map(|e| Item::on(today, Some(e.title_id))),
    );

    // Genres and decades, over titles in the library with something watched.
    let watched_entries: Vec<&Entry> = f
        .entries
        .iter()
        .filter(|e| watched_titles.contains(&e.title_id))
        .collect();
    let first_watch: HashMap<i64, &Watch> = firsts(watches.iter().copied(), |w| w.title_id)
        .into_iter()
        .map(|w| (w.title_id, w))
        .collect();
    let item_of = |e: &Entry| first_watch.get(&e.title_id).map(|w| Item::of(f, w));
    let in_genre = |g: &str| -> Vec<Item> {
        watched_entries
            .iter()
            .filter(|e| e.genres.iter().any(|x| x == g))
            .filter_map(|e| item_of(e))
            .collect()
    };
    once(&mut out, "brave-frog", nth(in_genre("Horror"), 25));
    once(&mut out, "laugh-track", nth(in_genre("Comedy"), 50));
    once(&mut out, "documentarian", nth(in_genre("Documentary"), 25));
    once(&mut out, "drawn-in", nth(in_genre("Animation"), 50));
    let mut genre_seen: BTreeMap<NaiveDate, Vec<(&String, i64)>> = BTreeMap::new();
    for e in &watched_entries {
        if let Some(i) = item_of(e) {
            for g in &e.genres {
                genre_seen.entry(i.on).or_default().push((g, e.title_id));
            }
        }
    }
    let mut genres = HashSet::new();
    for (on, list) in &genre_seen {
        for (g, title) in list {
            if genres.insert(*g) && genres.len() == 15 {
                once(&mut out, "genre-hopper", Some(Item::on(*on, Some(*title))));
            }
        }
    }
    // Where titles come from, and those not made in English.
    let mut country_seen: BTreeMap<(NaiveDate, i64), &Entry> = BTreeMap::new();
    for e in &watched_entries {
        if let Some(i) = item_of(e) {
            country_seen.insert((i.on, e.title_id), e);
        }
    }
    let mut countries = HashSet::new();
    for ((on, title), e) in &country_seen {
        for c in &e.countries {
            if countries.insert(c) && countries.len() == 15 {
                once(&mut out, "world-tour", Some(Item::on(*on, Some(*title))));
            }
        }
    }
    out.count("world-tour", countries.len() as f64, 15.0);
    let subtitled: Vec<Item> = watched_entries
        .iter()
        .filter(|e| !e.language.is_empty() && e.language != "en")
        .filter_map(|e| item_of(e))
        .collect();
    out.count("subtitles-on", subtitled.len() as f64, 25.0);
    once(&mut out, "subtitles-on", nth(subtitled, 25));
    let mut by_day: Vec<(NaiveDate, i64, i32)> = watched_entries
        .iter()
        .filter_map(|e| Some((item_of(e)?.on, e.title_id, e.release?.year() / 10)))
        .collect();
    by_day.sort();
    let mut decades = HashSet::new();
    for (on, title, decade) in by_day {
        if decades.insert(decade) {
            if decades.len() == 7 {
                once(&mut out, "time-traveller", Some(Item::on(on, Some(title))));
            }
            if decades.len() == 10 {
                once(&mut out, "century-leap", Some(Item::on(on, Some(title))));
            }
        }
    }
    once(
        &mut out,
        "silver-screen",
        watched_entries
            .iter()
            .filter(|e| e.kind == "movie" && e.release.is_some_and(|d| d.year() < 1960))
            .filter_map(|e| item_of(e))
            .min_by_key(|i| i.on),
    );

    // People and studios.
    for (key, role, least, films_only) in [
        ("directors-cut", "director", 5, true),
        ("fan-club", "cast", 10, false),
        ("studio-loyal", "studio", 15, false),
    ] {
        let mut by: HashMap<i64, Vec<Item>> = HashMap::new();
        for c in f.credits.iter().filter(|c| c.role == role) {
            let Some(e) = lib.get(&c.title_id) else {
                continue;
            };
            if films_only && e.kind != "movie" {
                continue;
            }
            if let Some(i) = item_of(e) {
                by.entry(c.person).or_default().push(i);
            }
        }
        once(
            &mut out,
            key,
            by.into_values()
                .filter_map(|items| nth(items, least))
                .min_by_key(|i| i.on),
        );
    }

    // One title, three times.
    let mut film_count: HashMap<i64, Vec<&Watch>> = HashMap::new();
    for w in &films {
        film_count.entry(w.title_id).or_default().push(w);
    }
    let comfort = film_count
        .values()
        .filter(|ws| ws.len() >= 3)
        .filter_map(|ws| nth(ws.iter().map(|w| Item::of(f, w)).collect(), 3))
        .chain(
            f.entries
                .iter()
                .filter(|e| e.kind == "tv" && e.rewatch_count >= 2)
                .map(|e| Item::on(e.finished_at.map_or(today, |t| f.day(t)), Some(e.title_id))),
        )
        .min_by_key(|i| i.on);
    once(&mut out, "comfort-show", comfort);

    if f.shelves >= 5 {
        once(&mut out, "curator", Some(Item::on(today, None)));
    }
    if f.tastes.iter().any(|p| *p >= 90) {
        once(&mut out, "taste-twin", Some(Item::on(today, None)));
    }
    if f.tastes.iter().any(|p| *p < 40) {
        once(&mut out, "opposites", Some(Item::on(today, None)));
    }
    once(
        &mut out,
        "pioneer",
        f.pioneered
            .iter()
            .map(|(t, d)| Item::on(*d, Some(*t)))
            .min_by_key(|i| i.on),
    );
    out
}

// --- moments -------------------------------------------------------------------

/// The watches moments read: eligible, and on a day that is over, both the
/// day watched and the day recorded.
fn finished_days<'a>(f: &'a Facts, eligible: &HashSet<i64>) -> Vec<&'a Watch> {
    let today = f.today();
    let mut out: Vec<&Watch> = f
        .watches
        .iter()
        .filter(|w| eligible.contains(&w.id))
        .filter(|w| f.day(w.watched_at) < today && w.logged_at.is_some_and(|t| f.day(t) < today))
        .collect();
    out.sort_by_key(|w| (w.watched_at, w.id));
    out
}

pub fn moments(f: &Facts, eligible: &HashSet<i64>) -> Tally {
    let mut out = Tally::default();
    let ws = finished_days(f, eligible);
    let lib: HashMap<i64, &Entry> = f.entries.iter().map(|e| (e.title_id, e)).collect();
    let local = |w: &Watch| w.watched_at.with_timezone(&f.tz);
    let day = |w: &Watch| f.day(w.watched_at);
    let give = |out: &mut Tally, key: &str, list: &[&Watch]| {
        if let Some(last) = list.last() {
            out.push(earned(
                key,
                day(last),
                Some(last.title_id),
                list.iter().map(|w| w.id).collect(),
            ));
        }
    };

    // A day's films and a day's episodes of one show.
    let mut films_by_day: BTreeMap<NaiveDate, Vec<&Watch>> = BTreeMap::new();
    let mut show_day: BTreeMap<(NaiveDate, i64), Vec<&Watch>> = BTreeMap::new();
    for w in &ws {
        if w.episode_id.is_none() {
            let list = films_by_day.entry(day(w)).or_default();
            if !list.iter().any(|x| x.title_id == w.title_id) {
                list.push(w);
            }
        } else {
            show_day.entry((day(w), w.title_id)).or_default().push(w);
        }
    }
    for (key, n) in [
        ("double-feature", 2),
        ("triple-bill", 3),
        ("film-festival", 5),
    ] {
        if let Some(list) = films_by_day.values().find(|l| l.len() >= n) {
            give(&mut out, key, &list[..n]);
        }
    }
    for (key, n) in [("marathon", 6), ("ultramarathon", 12)] {
        if let Some(list) = show_day.values().find(|l| l.len() >= n) {
            give(&mut out, key, &list[..n]);
        }
    }

    // Seasons and whole shows, from their episodes.
    let first_ok: HashMap<i64, &Watch> =
        firsts(ws.iter().copied().filter(|w| w.episode_id.is_some()), |w| {
            w.episode_id.unwrap_or_default()
        })
        .into_iter()
        .map(|w| (w.episode_id.unwrap_or_default(), w))
        .collect();
    let any_ok: HashMap<i64, Vec<&Watch>> =
        ws.iter()
            .filter(|w| w.episode_id.is_some())
            .fold(HashMap::new(), |mut m, w| {
                m.entry(w.episode_id.unwrap_or_default())
                    .or_default()
                    .push(*w);
                m
            });
    let mut seasons: BTreeMap<(i64, i64), Vec<&Episode>> = BTreeMap::new();
    for e in f.episodes.iter().filter(|e| e.released) {
        seasons.entry((e.title_id, e.season)).or_default().push(e);
    }

    // A whole season on one day.
    'sitting: for eps in seasons.values().filter(|eps| eps.len() >= 3) {
        let Some(list) = any_ok.get(&eps[0].id) else {
            continue;
        };
        for w in list {
            let d = day(w);
            let all: Option<Vec<&Watch>> = eps
                .iter()
                .map(|e| {
                    any_ok
                        .get(&e.id)
                        .and_then(|l| l.iter().find(|x| day(x) == d).copied())
                })
                .collect();
            if let Some(all) = all {
                give(&mut out, "one-sitting", &all);
                break 'sitting;
            }
        }
    }

    // Each episode of a season on its air day, or within days of it.
    let kept_up = |eps: &[&Episode], days: i64| -> Option<Vec<&Watch>> {
        eps.iter()
            .map(|e| {
                let air = e.air_day?;
                any_ok
                    .get(&e.id)?
                    .iter()
                    .find(|w| {
                        let d = day(w);
                        d >= air && d <= air + Duration::days(days)
                    })
                    .copied()
            })
            .collect()
    };
    if let Some(list) = seasons
        .values()
        .filter(|eps| eps.len() >= 4)
        .find_map(|eps| kept_up(eps, 0))
    {
        give(&mut out, "season-ticket", &list);
    }
    if let Some(list) = seasons
        .iter()
        .filter(|((t, _), eps)| eps.len() >= 6 && lib.get(t).is_some_and(|e| e.anime))
        .find_map(|(_, eps)| kept_up(eps, 7))
    {
        give(&mut out, "cour-keeper", &list);
    }

    // Whole shows.
    let mut shows: BTreeMap<i64, Vec<&Episode>> = BTreeMap::new();
    for ((t, _), eps) in &seasons {
        shows.entry(*t).or_default().extend(eps.iter().copied());
    }
    let whole = |eps: &[&Episode]| -> Option<Vec<&Watch>> {
        let mut list: Vec<&Watch> = eps
            .iter()
            .map(|e| first_ok.get(&e.id).copied())
            .collect::<Option<_>>()?;
        list.sort_by_key(|w| (w.watched_at, w.id));
        Some(list)
    };
    let span = |list: &[&Watch]| list.last().unwrap().watched_at - list.first().unwrap().watched_at;
    for (key, least_eps, least_seasons, within) in
        [("speedrun", 6, 1, 7), ("metamorphosis", 1, 5, 30)]
    {
        let found = shows.iter().find_map(|(t, eps)| {
            let season_count = seasons.keys().filter(|(x, _)| x == t).count();
            if eps.len() < least_eps || season_count < least_seasons {
                return None;
            }
            whole(eps).filter(|list| span(list) <= Duration::days(within))
        });
        if let Some(list) = found {
            give(&mut out, key, &list);
        }
    }
    let followed = shows.iter().find_map(|(t, eps)| {
        let ended = lib
            .get(t)
            .is_some_and(|e| matches!(e.tmdb_status.as_deref(), Some("Ended" | "Canceled")));
        if !ended || eps.len() < 6 {
            return None;
        }
        kept_up(eps, 7)
    });
    if let Some(list) = followed {
        give(&mut out, "tadpole-to-frog", &list);
    }

    // A hundred episodes in a week.
    let eps_only: Vec<&Watch> = ws
        .iter()
        .copied()
        .filter(|w| w.episode_id.is_some())
        .collect();
    for (i, w) in eps_only.iter().enumerate() {
        if i >= 99 && w.watched_at - eps_only[i - 99].watched_at <= Duration::days(7) {
            give(&mut out, "fly-feast", &eps_only[i - 99..=i]);
            break;
        }
    }

    // The hours of the night, and of the clock.
    let night = |w: &&&Watch| (2..5).contains(&local(w).hour());
    if let Some(w) = ws.iter().find(night) {
        give(&mut out, "all-nighter", &[*w]);
    }
    let mut nights: BTreeMap<NaiveDate, &Watch> = BTreeMap::new();
    for w in ws.iter().filter(night) {
        nights.entry(day(w)).or_insert(w);
    }
    if nights.len() >= 10 {
        let list: Vec<&Watch> = nights.values().take(10).copied().collect();
        give(&mut out, "nocturnal", &list);
    }
    let mut hours: BTreeMap<u32, &Watch> = BTreeMap::new();
    for w in &ws {
        hours.entry(local(w).hour()).or_insert(w);
        if hours.len() == 24 {
            let mut list: Vec<&Watch> = hours.values().copied().collect();
            list.sort_by_key(|w| w.watched_at);
            give(&mut out, "croak-around-the-clock", &list);
            break;
        }
    }

    // Every weekend day of a month that is over.
    let days: BTreeMap<NaiveDate, &Watch> = ws.iter().fold(BTreeMap::new(), |mut m, w| {
        m.entry(day(w)).or_insert(*w);
        m
    });
    let months: HashSet<(i32, u32)> = days.keys().map(|d| (d.year(), d.month())).collect();
    let mut months: Vec<_> = months.into_iter().collect();
    months.sort();
    for (y, m) in months {
        let Some(start) = NaiveDate::from_ymd_opt(y, m, 1) else {
            continue;
        };
        let end = start
            .checked_add_months(chrono::Months::new(1))
            .unwrap_or(start);
        if end > f.today() {
            continue;
        }
        let weekend: Vec<NaiveDate> = start
            .iter_days()
            .take_while(|d| *d < end)
            .filter(|d| matches!(d.weekday(), Weekday::Sat | Weekday::Sun))
            .collect();
        if let Some(list) = weekend
            .iter()
            .map(|d| days.get(d).copied())
            .collect::<Option<Vec<_>>>()
        {
            give(&mut out, "weekend-warrior", &list);
            break;
        }
    }

    // On its day.
    let episode: HashMap<i64, &Episode> = f.episodes.iter().map(|e| (e.id, e)).collect();
    let ep_of = |w: &Watch| w.episode_id.and_then(|id| episode.get(&id).copied());
    if let Some(w) = ws
        .iter()
        .find(|w| ep_of(w).and_then(|e| e.air_day) == Some(day(w)))
    {
        give(&mut out, "day-one", &[*w]);
    }
    let within = |w: &Watch, hours: i64| {
        ep_of(w)
            .and_then(|e| e.air_at)
            .is_some_and(|at| w.watched_at >= at && w.watched_at - at <= Duration::hours(hours))
    };
    if let Some(w) = ws.iter().find(|w| within(w, 3)) {
        give(&mut out, "live-tonight", &[*w]);
    }
    if let Some(w) = ws
        .iter()
        .find(|w| within(w, 24) && lib.get(&w.title_id).is_some_and(|e| e.anime))
    {
        give(&mut out, "simulcast", &[*w]);
    }

    // Films around their release, and titles kept a long time.
    let film_ws: Vec<&Watch> = ws
        .iter()
        .copied()
        .filter(|w| w.episode_id.is_none())
        .collect();
    let release_week = |w: &Watch| {
        lib.get(&w.title_id)
            .and_then(|e| e.release)
            .is_some_and(|r| day(w) >= r && day(w) <= r + Duration::days(7))
    };
    if let Some(w) = film_ws.iter().find(|w| release_week(w)) {
        give(&mut out, "premiere-night", &[*w]);
    }
    if let Some(w) = film_ws.iter().find(|w| {
        release_week(w)
            && lib.get(&w.title_id).is_some_and(
                |e| matches!((e.added_at, e.release), (Some(a), Some(r)) if f.day(a) < r),
            )
    }) {
        give(&mut out, "early-riser", &[*w]);
    }
    let first_ever: HashMap<i64, i64> = firsts(f.watches.iter(), |w| w.title_id)
        .into_iter()
        .map(|w| (w.title_id, w.id))
        .collect();
    if let Some(w) = ws.iter().find(|w| {
        first_ever.get(&w.title_id) == Some(&w.id)
            && lib
                .get(&w.title_id)
                .and_then(|e| e.added_at)
                .is_some_and(|a| w.watched_at - a >= Duration::days(365))
    }) {
        give(&mut out, "patience", &[*w]);
    }

    // The calendar.
    let mut octobers: BTreeMap<i32, Vec<&Watch>> = BTreeMap::new();
    let mut summers: BTreeMap<i32, Vec<&Watch>> = BTreeMap::new();
    for w in &ws {
        let d = day(w);
        let horror = lib
            .get(&w.title_id)
            .is_some_and(|e| e.genres.iter().any(|g| g == "Horror"));
        if d.month() == 10 && horror {
            let l = octobers.entry(d.year()).or_default();
            if !l.iter().any(|x| x.title_id == w.title_id) {
                l.push(w);
            }
        }
        if (7..=8).contains(&d.month()) && w.episode_id.is_none() {
            let l = summers.entry(d.year()).or_default();
            if !l.iter().any(|x| x.title_id == w.title_id) {
                l.push(w);
            }
        }
    }
    if let Some(l) = octobers.values().find(|l| l.len() >= 10) {
        give(&mut out, "spooky-season", &l[..10]);
    }
    if let Some(l) = summers.values().find(|l| l.len() >= 10) {
        give(&mut out, "summer-blockbuster", &l[..10]);
    }
    if let Some((eve, new)) = days.iter().find_map(|(d, w)| {
        (d.month() == 12 && d.day() == 31)
            .then(|| days.get(&(*d + Duration::days(1))).map(|n| (*w, *n)))
            .flatten()
    }) {
        give(&mut out, "fireworks", &[eve, new]);
    }
    if let Some(w) = days
        .iter()
        .find(|(d, _)| d.month() == 2 && d.day() == 29)
        .map(|(_, w)| *w)
    {
        give(&mut out, "leap-year", &[w]);
    }

    // Back after two months: the watch before it, of any kind, is that far back.
    let mut dated: Vec<&Watch> = f
        .watches
        .iter()
        .filter(|w| !w.undated && w.watched_at <= f.now)
        .collect();
    dated.sort_by_key(|w| (w.watched_at, w.id));
    let place: HashMap<i64, usize> = dated.iter().enumerate().map(|(i, w)| (w.id, i)).collect();
    if let Some(w) = ws.iter().find(|w| {
        place.get(&w.id).is_some_and(|i| {
            *i > 0 && w.watched_at - dated[*i - 1].watched_at >= Duration::days(60)
        })
    }) {
        give(&mut out, "spring-thaw", &[*w]);
    }

    // Streaks of days in a row.
    let mut run: Vec<NaiveDate> = Vec::new();
    let mut best: HashMap<usize, NaiveDate> = HashMap::new();
    for d in days.keys() {
        if run.last().is_some_and(|l| *l + Duration::days(1) != *d) {
            run.clear();
        }
        run.push(*d);
        best.entry(run.len()).or_insert(*d);
    }
    let longest = best.keys().max().copied().unwrap_or(0);
    for step in catalogue::ladder("streak") {
        let at = step.step.as_ref().map_or(0, |s| s.at) as usize;
        out.count(&step.key, longest as f64, at as f64);
        if let Some(on) = best.get(&at) {
            out.push(earned(
                &step.key,
                *on,
                days.get(on).map(|w| w.title_id),
                days.get(on).map(|w| vec![w.id]).unwrap_or_default(),
            ));
        }
    }
    out
}

// --- badges about badges -------------------------------------------------------

/// Badges that count other badges: everything held after this run, the
/// hand-made ones too.
pub fn meta(held: &HashSet<String>, hand_made: usize, today: NaiveDate) -> Vec<Earned> {
    let mut out = Vec::new();
    let total = held.len() + hand_made;
    for (key, least) in [("king-of-the-pond", 50), ("pond-legend", 100)] {
        if total >= least {
            out.push(earned(key, today, None, Vec::new()));
        }
    }
    let steps: Vec<&catalogue::Step> = held
        .iter()
        .filter_map(|k| catalogue::get(k)?.step.as_ref())
        .collect();
    if steps.iter().any(|s| s.last) {
        out.push(earned("summit-frog", today, None, Vec::new()));
    }
    let gold: HashSet<&str> = steps
        .iter()
        .filter(|s| s.metal.is_some_and(|m| m >= 2))
        .map(|s| s.ladder.as_str())
        .collect();
    if gold.len() >= 5 {
        out.push(earned("frog-prince", today, None, Vec::new()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::awards::facts::Change;

    fn at(s: &str) -> DateTime<Utc> {
        crate::awards::eligible::parse(s).unwrap()
    }

    fn facts(entries: Vec<Entry>, watches: Vec<Watch>, changes: Vec<Change>) -> Facts {
        Facts {
            tz: chrono_tz::Tz::UTC,
            now: at("2026-11-30T12:00:00Z"),
            watches,
            entries,
            episodes: Vec::new(),
            reads: Vec::new(),
            credits: Vec::new(),
            shelves: 0,
            joined: None,
            tastes: Vec::new(),
            pioneered: Vec::new(),
            changes,
        }
    }

    fn entry(title_id: i64, kind: &str, status: &str) -> Entry {
        Entry {
            title_id,
            kind: kind.into(),
            status: status.into(),
            ..Default::default()
        }
    }

    fn watch(id: i64, title_id: i64, when: &str) -> Watch {
        Watch {
            id,
            title_id,
            episode_id: None,
            season: None,
            watched_at: at(when),
            logged_at: Some(at(when)),
            how: Some("tick".into()),
            undated: false,
            is_rewatch: false,
            source: "manual".into(),
            minutes: 100.0,
            with_someone: false,
        }
    }

    fn change(title_id: i64, from: Option<&str>, to: Option<&str>, when: &str) -> Change {
        Change {
            title_id,
            from: from.map(Into::into),
            to: to.map(Into::into),
            at: at(when),
        }
    }

    fn keys(t: &Tally) -> Vec<(&str, String)> {
        t.earned
            .iter()
            .map(|e| (e.key.as_str(), e.on.to_string()))
            .collect()
    }

    #[test]
    fn a_dropped_show_picked_up_and_watched_comes_back_from_the_dead() {
        let f = facts(
            vec![entry(1, "tv", "watching")],
            vec![watch(7, 1, "2026-11-10T20:30:00Z")],
            vec![change(
                1,
                Some("dropped"),
                Some("watching"),
                "2026-11-10T20:00:00Z",
            )],
        );
        assert_eq!(
            keys(&status_history(&f, &HashSet::from([7]))),
            vec![("back-from-the-dead", "2026-11-10".to_string())]
        );
        // A watch marked in bulk, or no watch at all, says nothing.
        assert!(status_history(&f, &HashSet::new()).earned.is_empty());
    }

    #[test]
    fn a_watchlist_of_twenty_watched_down_to_nothing_is_watchlist_zero() {
        let mut entries = Vec::new();
        let mut watches = Vec::new();
        let mut changes = Vec::new();
        for n in 0..20 {
            let title = 100 + n;
            let day = format!("2026-11-{:02}T20:00:00Z", 2 + n);
            entries.push(entry(title, "movie", "completed"));
            changes.push(change(title, None, Some("planned"), "2026-11-01T10:00:00Z"));
            changes.push(change(title, Some("planned"), Some("completed"), &day));
            watches.push(watch(title, title, &day));
        }
        changes.sort_by_key(|c| c.at);
        let every: HashSet<i64> = watches.iter().map(|w| w.id).collect();
        let f = facts(entries.clone(), watches.clone(), changes.clone());
        assert_eq!(
            keys(&status_history(&f, &every)),
            vec![("watchlist-zero", "2026-11-21".to_string())]
        );

        // Emptied by removing the last one instead: not yet.
        let last = changes.last_mut().unwrap();
        last.to = None;
        entries.pop();
        let f = facts(entries, watches, changes);
        assert!(status_history(&f, &every).earned.is_empty());
    }
}
