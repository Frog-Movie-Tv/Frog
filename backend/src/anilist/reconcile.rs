//! Who wins, entry by entry. No database and no network: every decision the
//! two-way sync makes about status, progress and score is here, on plain
//! values, so it can be tested on its own.
//!
//! Each entry keeps a snapshot of both sides from the last sync. A side that
//! differs from its snapshot changed; when both changed, whoever is further
//! along wins. A first sync (or a re-placed entry) only adds: it never takes
//! anything away from either side.

use super::client::View;
use crate::transfer::merged_status;

/// How firmly AniList's side is applied to Frog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// First sync of an entry: only add and move forward.
    Merge,
    /// AniList changed since the last sync: it is the truth.
    Follow,
}

pub fn frog_to_anilist(status: &str) -> &'static str {
    match status {
        "watching" => "CURRENT",
        "completed" => "COMPLETED",
        "paused" => "PAUSED",
        "dropped" => "DROPPED",
        _ => "PLANNING",
    }
}

pub fn anilist_to_frog(status: &str) -> &'static str {
    match status {
        "CURRENT" | "REPEATING" => "watching",
        "COMPLETED" => "completed",
        "PAUSED" => "paused",
        "DROPPED" => "dropped",
        _ => "planned",
    }
}

/// What changed on each side since the last sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Changes {
    /// Never synced (or synced somewhere else): nothing to compare against.
    pub first: bool,
    pub al_changed: bool,
    pub frog_changed: bool,
    /// AniList's side is applied: it changed, and Frog did not, or Frog did
    /// but AniList is at least as far along.
    pub follow: bool,
}

impl Changes {
    /// `snap_al` and `snap_frog` are the snapshots from the last sync;
    /// `al` and `frog` are each side now (`frog` is None when the title is
    /// not in the library).
    pub fn between(
        snap_al: Option<&View>,
        snap_frog: Option<&View>,
        al: &View,
        frog: Option<&View>,
    ) -> Self {
        let first = snap_al.is_none();
        let al_changed = !first && snap_al != Some(al);
        let frog_changed = !first && frog != snap_frog;
        let frog_progress = frog.map(|f| f.progress).unwrap_or(0);
        Self {
            first,
            al_changed,
            frog_changed,
            follow: al_changed && (!frog_changed || al.progress >= frog_progress),
        }
    }

    /// How AniList's side is applied to an entry Frog can tick episodes of,
    /// or None when Frog's side stands. `replaced` is an entry a newer
    /// matcher placed again, which starts afresh like a first sync.
    pub fn mode(&self, replaced: bool) -> Option<Mode> {
        if self.first || replaced {
            Some(Mode::Merge)
        } else if self.follow {
            Some(Mode::Follow)
        } else {
            None
        }
    }
}

/// The rating AniList's score asks for, if it should change: `Some(new)`,
/// where `new` may be None (the score was taken off). A merge only fills an
/// empty rating; following takes AniList's, including its removal.
pub fn incoming_rating(
    current: Option<f64>,
    score: i64,
    mode: Mode,
    before: Option<&View>,
) -> Option<Option<f64>> {
    if score > 0 {
        match mode {
            Mode::Merge => current.is_none().then_some(Some(score as f64)),
            Mode::Follow => Some(Some(score as f64)),
        }
    } else if mode == Mode::Follow && before.is_some_and(|b| b.score > 0) {
        Some(None)
    } else {
        None
    }
}

/// A film's (or AniList-only title's) status from AniList's word: new to the
/// library takes it outright, a merge only moves forward, following takes it.
pub fn incoming_status(existing: Option<&str>, incoming: &str, mode: Mode) -> String {
    match existing {
        Some(have) if mode == Mode::Merge => merged_status(have, incoming),
        _ => incoming.to_string(),
    }
}

/// A film as AniList should see it: seen once there is a watch or it is
/// completed, whatever else the status says.
pub fn film_view(status: &str, rating: Option<f64>, watches: i64) -> View {
    let done = status == "completed" || watches > 0;
    View {
        status: if done {
            "COMPLETED".into()
        } else {
            frog_to_anilist(status).into()
        },
        progress: i64::from(done),
        score: score_of(rating),
    }
}

/// A season (or part) as AniList should see it, from the episodes of it
/// watched in Frog. Paused and dropped are the person's word until the part
/// is finished; otherwise the progress says it.
pub fn show_view(status: &str, progress: i64, episodes: Option<i64>, rating: Option<f64>) -> View {
    let finished = episodes.is_some_and(|n| n > 0 && progress >= n);
    let status = match status {
        "paused" | "dropped" if !finished => frog_to_anilist(status),
        _ if finished => "COMPLETED",
        _ if progress > 0 => "CURRENT",
        _ => "PLANNING",
    };
    View {
        status: status.into(),
        progress,
        score: score_of(rating),
    }
}

/// A 0–100 rating as AniList scores it, 0 being none.
pub fn score_of(rating: Option<f64>) -> i64 {
    rating.map(|r| r.round() as i64).unwrap_or(0)
}

/// A show's one status from the AniList entries that make it up: `(season,
/// offset, AniList status)` each. The last entry started decides paused or
/// dropped; anything else leaves the status to what has come out
/// (`release::reconcile`), except that following AniList takes a show back
/// up when it was paused or dropped in Frog.
pub fn show_status(current: &str, entries: &[(i64, i64, String)], mode: Mode) -> String {
    let mut ordered = entries.to_vec();
    ordered.sort_by_key(|(s, o, _)| (*s, *o));
    let last_started = ordered.iter().rposition(|(_, _, st)| st != "PLANNING");
    let word = last_started.and_then(|i| match ordered[i].2.as_str() {
        "PAUSED" => Some("paused"),
        "DROPPED" => Some("dropped"),
        _ => None,
    });
    match (word, mode) {
        (Some(w), Mode::Merge) => merged_status(current, w),
        (Some(w), Mode::Follow) => w.to_string(),
        (None, Mode::Follow) if matches!(current, "paused" | "dropped") => "watching".to_string(),
        _ => current.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(status: &str, progress: i64, score: i64) -> View {
        View {
            status: status.into(),
            progress,
            score,
        }
    }

    #[test]
    fn a_first_sync_merges() {
        let al = v("CURRENT", 3, 80);
        let frog = v("CURRENT", 5, 0);
        let c = Changes::between(None, None, &al, Some(&frog));
        assert!(c.first && !c.al_changed && !c.frog_changed && !c.follow);
        assert_eq!(c.mode(false), Some(Mode::Merge));
    }

    #[test]
    fn a_change_on_anilist_comes_in_and_one_in_frog_stands() {
        let snap = v("CURRENT", 3, 0);
        // AniList moved on; Frog did not.
        let c = Changes::between(Some(&snap), Some(&snap), &v("CURRENT", 5, 0), Some(&snap));
        assert!(c.follow);
        assert_eq!(c.mode(false), Some(Mode::Follow));

        // Frog moved on; AniList did not: Frog's side stands.
        let now = v("CURRENT", 4, 70);
        let al = v("CURRENT", 3, 0);
        let before = v("CURRENT", 3, 70);
        let c = Changes::between(Some(&al), Some(&before), &al, Some(&now));
        assert!(c.frog_changed && !c.al_changed && !c.follow);
        assert_eq!(c.mode(false), None);
    }

    #[test]
    fn when_both_sides_moved_the_one_further_along_wins() {
        let snap = v("CURRENT", 3, 0);
        let ahead = Changes::between(
            Some(&snap),
            Some(&snap),
            &v("CURRENT", 6, 0),
            Some(&v("CURRENT", 5, 0)),
        );
        assert!(ahead.follow);
        let behind = Changes::between(
            Some(&snap),
            Some(&snap),
            &v("CURRENT", 4, 0),
            Some(&v("CURRENT", 5, 0)),
        );
        assert!(!behind.follow);
    }

    #[test]
    fn a_title_taken_out_of_frog_counts_as_a_frog_change() {
        let snap = v("CURRENT", 3, 0);
        let c = Changes::between(Some(&snap), Some(&snap), &snap, None);
        assert!(c.frog_changed && !c.follow);
    }

    #[test]
    fn ratings_merge_into_empty_follow_outright_and_follow_a_removal() {
        assert_eq!(
            incoming_rating(None, 80, Mode::Merge, None),
            Some(Some(80.0))
        );
        assert_eq!(incoming_rating(Some(60.0), 80, Mode::Merge, None), None);
        assert_eq!(
            incoming_rating(Some(60.0), 80, Mode::Follow, None),
            Some(Some(80.0))
        );
        let scored = v("CURRENT", 1, 70);
        assert_eq!(
            incoming_rating(Some(70.0), 0, Mode::Follow, Some(&scored)),
            Some(None)
        );
        assert_eq!(
            incoming_rating(Some(70.0), 0, Mode::Merge, Some(&scored)),
            None
        );
        assert_eq!(incoming_rating(None, 0, Mode::Follow, None), None);
    }

    #[test]
    fn statuses_merge_forward_and_follow_outright() {
        assert_eq!(incoming_status(None, "completed", Mode::Merge), "completed");
        assert_eq!(
            incoming_status(Some("paused"), "completed", Mode::Merge),
            "paused"
        );
        assert_eq!(
            incoming_status(Some("planned"), "watching", Mode::Merge),
            "watching"
        );
        assert_eq!(
            incoming_status(Some("completed"), "watching", Mode::Follow),
            "watching"
        );
    }

    #[test]
    fn frogs_side_reads_as_anilist_would_say_it() {
        assert_eq!(film_view("planned", Some(84.6), 1), v("COMPLETED", 1, 85));
        assert_eq!(film_view("paused", None, 0), v("PAUSED", 0, 0));
        assert_eq!(
            show_view("watching", 12, Some(12), None),
            v("COMPLETED", 12, 0)
        );
        assert_eq!(show_view("dropped", 4, Some(12), None), v("DROPPED", 4, 0));
        assert_eq!(
            show_view("dropped", 12, Some(12), None),
            v("COMPLETED", 12, 0)
        );
        assert_eq!(show_view("completed", 0, None, None), v("PLANNING", 0, 0));
        // Unknown length: never finished by counting.
        assert_eq!(
            show_view("watching", 30, None, Some(90.0)),
            v("CURRENT", 30, 90)
        );
    }

    #[test]
    fn a_show_takes_the_word_of_its_last_started_season() {
        let e = |s: i64, st: &str| (s, 0, st.to_string());
        let seasons = [e(2, "PLANNING"), e(1, "PAUSED")];
        assert_eq!(show_status("watching", &seasons, Mode::Follow), "paused");
        // A merge never undoes a later status of yours.
        assert_eq!(show_status("completed", &seasons, Mode::Merge), "completed");
        assert_eq!(show_status("planned", &seasons, Mode::Merge), "planned");
        // Picked up again on AniList.
        let going = [e(1, "COMPLETED"), e(2, "CURRENT")];
        assert_eq!(show_status("dropped", &going, Mode::Follow), "watching");
        assert_eq!(show_status("dropped", &going, Mode::Merge), "dropped");
        assert_eq!(show_status("completed", &going, Mode::Follow), "completed");
    }
}
