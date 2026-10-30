//! One sync of one account, start to finish, and when the worker runs one.
//!
//! Read the whole list; place each entry (or keep the place it has); then,
//! entry by entry, carry across whichever side changed (`reconcile`). Shows
//! are settled once at the end, when all of their seasons are in. A failure
//! that is about one entry is counted and the rest go on; an outage (TMDB or
//! AniList out of reach) stops the sync, which is tried again later.

use std::collections::{HashMap, HashSet};

use chrono::Utc;
use serde::Serialize;
use serde_json::{json, Value};

use super::client::{self, push, Api, Remote};
use super::idmap::{ensure_map, remember_media};
use super::library::{apply, frog_view, settle_show, unplace};
use super::local::{adopt_local, next_try, sync_local};
use super::place::{self, Outcome, Placement, Target, CONFIDENT, RESOLVER};
use super::reconcile::{self, Changes, Mode};
use super::records::{self, save_entry, save_snapshots, Known, Verdict};
use super::Account;
use crate::catalog::{self, ExternalId};
use crate::error::{AppError, AppResult};
use crate::release;
use crate::state::AppState;

/// How often the worker re-reads a list nobody touched, in minutes: one
/// request per 500 entries, so a change made on AniList (an episode ticked
/// by a streaming site, say) shows up in Frog within minutes without Frog
/// being open. `ANILIST_SYNC_MINUTES` sets it, between 1 and 60.
pub fn sync_minutes() -> i64 {
    static MINUTES: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *MINUTES.get_or_init(|| {
        std::env::var("ANILIST_SYNC_MINUTES")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .unwrap_or(3)
            .clamp(1, 60)
    })
}

/// The longest a failing account waits before it is tried again.
const MAX_BACKOFF_MINUTES: i64 = 60;

/// Accounts with a sync running, so two never overlap.
static RUNNING: std::sync::Mutex<Option<HashSet<i64>>> = std::sync::Mutex::new(None);

pub fn is_syncing(user_id: i64) -> bool {
    RUNNING
        .lock()
        .map(|g| g.as_ref().is_some_and(|s| s.contains(&user_id)))
        .unwrap_or(false)
}

struct RunningGuard(i64);

impl RunningGuard {
    fn claim(user_id: i64) -> Option<Self> {
        let mut guard = RUNNING.lock().ok()?;
        let set = guard.get_or_insert_with(HashSet::new);
        // Built only once the lock is let go: dropping a guard takes the lock.
        if !set.insert(user_id) {
            return None;
        }
        drop(guard);
        Some(Self(user_id))
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        if let Ok(mut guard) = RUNNING.lock() {
            if let Some(set) = guard.as_mut() {
                set.remove(&self.0);
            }
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub struct Summary {
    pub to_frog: i64,
    pub to_anilist: i64,
    pub unsure: i64,
    pub entries: i64,
    /// Not out yet, and not on TMDB yet: placed once they are.
    #[serde(default)]
    pub upcoming: i64,
    /// Placements moved to a better place this sync.
    #[serde(default)]
    pub repaired: i64,
    /// Kept in the library from AniList alone, with no TMDB title yet.
    #[serde(default)]
    pub local: i64,
    /// Entries that could not be written this time; tried again next sync.
    /// A sync with any is finished, but not complete.
    #[serde(default)]
    pub failed: i64,
    /// No longer on the AniList list. Kept in Frog as they are.
    #[serde(default)]
    pub removed: i64,
}

/// Accounts whose list is due: changed in Frog, never synced, or idle long
/// enough to look again, and not waiting out a failure.
pub async fn due(pool: &sqlx::SqlitePool) -> AppResult<Vec<i64>> {
    records::due(pool, sync_minutes()).await
}

/// Where each entry would go, how sure that is and why, without writing to
/// anyone's library: (media id, name, outcome, place, method, confidence,
/// note).
pub async fn diagnose(state: &AppState, remotes: &[Remote]) -> AppResult<Vec<Value>> {
    let api = Api::new(state)?;
    if let Some(client) = state.config.anilist.as_ref() {
        ensure_map(state, client).await?;
    }
    let mut out = Vec::new();
    for remote in remotes {
        let row = match place::place(state, &api, "", remote).await {
            Ok(p) => json!({
                "media_id": remote.media_id,
                "name": remote.name,
                "format": remote.format,
                "media_status": remote.media_status,
                "outcome": p.outcome.as_str(),
                "place": p.target.map(|t| match t {
                    Target::Movie(id) => format!("film {id}"),
                    Target::Show { tmdb_id, season, offset } => {
                        format!("show {tmdb_id} S{season} from E{}", offset + 1)
                    }
                }),
                "method": p.method,
                "confidence": p.confidence,
                "note": p.note,
                "candidates": p.candidates,
                "evidence": p.evidence,
            }),
            Err(e) => json!({
                "media_id": remote.media_id,
                "name": remote.name,
                "outcome": "error",
                "note": e.to_string(),
            }),
        };
        out.push(row);
    }
    Ok(out)
}

/// Sync one account, start to finish. Records the outcome on the link.
pub async fn sync_user(state: &AppState, user_id: i64) -> AppResult<Summary> {
    let Some(_running) = RunningGuard::claim(user_id) else {
        return Ok(Summary::default());
    };
    let result = sync_inner(state, user_id).await;
    let now = Utc::now().to_rfc3339();
    match &result {
        // Only a sync that finished moves synced_at: it is the checkpoint
        // Settings shows and the worker counts from.
        Ok(summary) => {
            records::sync_succeeded(&state.pool, user_id, &now, &serde_json::to_string(summary)?)
                .await?;
        }
        Err(e) => {
            let signed_out = matches!(e, AppError::Forbidden(_));
            let failures = records::failures(&state.pool, user_id).await?;
            let retry_at = retry_after(failures);
            tracing::warn!(user_id, failures = failures + 1, signed_out, error = %e, "AniList sync failed");
            records::sync_failed(
                &state.pool,
                user_id,
                &now,
                &failure_message(e),
                &retry_at,
                signed_out,
            )
            .await?;
        }
    }
    result
}

/// What Settings says about a failed sync. Something the person can act on
/// keeps its own words; an outage is said plainly.
fn failure_message(e: &AppError) -> String {
    match e {
        AppError::Forbidden(m) | AppError::BadRequest(m) => m.clone(),
        AppError::Tmdb(_) | AppError::TmdbUnconfigured => {
            "Could not reach TMDB to place your titles. Frog will try again shortly.".to_string()
        }
        _ => "Could not reach AniList. Frog will try again shortly.".to_string(),
    }
}

/// When a failing account is tried again: one minute, doubling up to an
/// hour, give or take a fifth so accounts do not move in step.
fn retry_after(failures: i64) -> String {
    let minutes = (1i64 << failures.clamp(0, 6)).min(MAX_BACKOFF_MINUTES) as f64;
    let jitter: f64 = {
        use rand::Rng;
        rand::thread_rng().gen_range(0.8..1.2)
    };
    let seconds = (minutes * 60.0 * jitter) as i64;
    (Utc::now() + chrono::Duration::seconds(seconds)).to_rfc3339()
}

/// TMDB out of reach, as opposed to something wrong with one entry: the
/// whole sync stops and is tried again, so nothing is marked failed for it.
fn is_outage(e: &AppError) -> bool {
    matches!(
        e,
        AppError::Tmdb(_) | AppError::TmdbUnconfigured | AppError::AniList(_)
    )
}

async fn sync_inner(state: &AppState, user_id: i64) -> AppResult<Summary> {
    let api = Api::new(state)?;
    let Some((anilist_user_id, token)) = records::link_token(&state.pool, user_id).await? else {
        return Ok(Summary::default());
    };
    ensure_map(state, api.client).await?;

    let remotes = client::read_list(&api, &token, anilist_user_id).await?;
    for r in &remotes {
        remember_media(
            &state.pool,
            r.media_id,
            r.episodes,
            r.format.as_deref(),
            &r.name,
        )
        .await?;
    }
    let known = records::known_entries(&state.pool, user_id).await?;

    let mut run = Run {
        acct: Account {
            state,
            api: &api,
            token: &token,
            user_id,
        },
        started: Utc::now().to_rfc3339(),
        summary: Summary {
            entries: remotes.len() as i64,
            ..Default::default()
        },
        shows: HashMap::new(),
        touched: HashSet::new(),
    };
    run.summary.removed = records::mark_gone(&state.pool, user_id, &remotes, &known).await?;
    for remote in &remotes {
        run.entry(remote, known.get(&remote.media_id)).await?;
    }
    run.settle_shows(&remotes).await?;
    Ok(run.summary)
}

/// Where a placed entry goes this sync, and the verdict Settings shows.
struct Decision {
    target: Target,
    verdict: Verdict<'static>,
    /// Kept for a placement Frog could not confirm, so the person can check.
    candidates: Vec<Value>,
}

/// One sync in progress.
struct Run<'a> {
    acct: Account<'a>,
    /// When the sync started, to compare `next_try_at` against.
    started: String,
    summary: Summary,
    /// Shows changed from AniList's side, and how firmly.
    shows: HashMap<i64, Mode>,
    /// Every show an entry is placed in, so each is settled once at the end.
    touched: HashSet<i64>,
}

impl Run<'_> {
    /// One entry: placed and synced, kept as an AniList-only title, or left
    /// as it is. Only an outage (or the database) stops the whole sync.
    async fn entry(&mut self, remote: &Remote, prior: Option<&Known>) -> AppResult<()> {
        if prior.is_some_and(|k| k.state == "ignored" || k.detached) {
            return Ok(());
        }
        let prior_place =
            prior.and_then(|k| k.target.zip(k.title_id).filter(|_| k.state == "placed"));

        // Not placed last time and nothing new to go on: it stays as it is in
        // the library until its next look, which keeps a short sync short.
        if prior.is_some_and(|k| k.wait_to_place(remote, &self.started)) {
            return self.keep_local(remote, prior).await;
        }
        match self.decide(remote, prior, prior_place).await? {
            Some(decision) => self.sync_placed(remote, prior, prior_place, decision).await,
            None => Ok(()),
        }
    }

    /// Where the entry goes. None when it was dealt with here: it could not
    /// be looked up, or it is not placed and is kept as it is.
    async fn decide(
        &mut self,
        remote: &Remote,
        prior: Option<&Known>,
        prior_place: Option<(Target, i64)>,
    ) -> AppResult<Option<Decision>> {
        // A place the person chose, or one the current matcher made, stays.
        // Anything older is looked at again and moved only when the new
        // place is certain.
        let keep = prior.is_some_and(|k| k.manual || k.resolver >= RESOLVER);
        if let (Some((target, _)), true) = (prior_place, keep) {
            return Ok(Some(Decision {
                target,
                verdict: Verdict {
                    state: "placed",
                    outcome: "matched",
                    method: None,
                    // A kept verdict is not rewritten (see `sync_placed`).
                    confidence: -1,
                    note: None,
                },
                candidates: Vec::new(),
            }));
        }

        let a = &self.acct;
        let p = match place::place(a.state, a.api, a.token, remote).await {
            Ok(p) => p,
            Err(e) if is_outage(&e) => return Err(e),
            Err(e) => {
                // One entry Frog cannot look up does not hold up the rest; it
                // is looked at again next time.
                tracing::warn!(media_id = remote.media_id, error = %e, "placing an AniList entry failed");
                self.summary.failed += 1;
                return Ok(None);
            }
        };
        match (p.target, prior_place) {
            (Some(target), _) => Ok(Some(Decision {
                target,
                verdict: Verdict {
                    state: "placed",
                    outcome: "matched",
                    method: p.method,
                    confidence: p.confidence,
                    note: p.note,
                },
                candidates: Vec::new(),
            })),
            // Placed before, and the matcher can no longer say: leave it be
            // rather than take progress away.
            (None, Some((old, _))) => Ok(Some(Decision {
                target: old,
                verdict: Verdict {
                    state: "placed",
                    outcome: "matched",
                    method: Some("kept"),
                    confidence: p.confidence.min(CONFIDENT - 1),
                    note: Some("Kept where it was. Frog could not confirm it, so check it.".into()),
                },
                candidates: p.candidates,
            })),
            (None, None) => {
                self.leave_unplaced(remote, prior, p).await?;
                Ok(None)
            }
        }
    }

    /// Not placed: say why, look again later (sooner when it is not out yet,
    /// and at once if AniList changes it), and keep it as AniList has it.
    async fn leave_unplaced(
        &mut self,
        remote: &Remote,
        prior: Option<&Known>,
        p: Placement,
    ) -> AppResult<()> {
        let pool = &self.acct.state.pool;
        let user_id = self.acct.user_id;
        match p.outcome {
            Outcome::Upcoming => self.summary.upcoming += 1,
            _ => self.summary.unsure += 1,
        }
        let show_title = match p.show {
            Some(id) => catalog::find(pool, &ExternalId::tmdb("tv", id)).await?,
            None => None,
        };
        let verdict = Verdict {
            state: "unsure",
            outcome: p.outcome.as_str(),
            method: p.method,
            confidence: p.confidence,
            note: p.note,
        };
        save_entry(pool, user_id, remote, &verdict, None, Some(&p.candidates)).await?;
        if let Some(t) = show_title {
            records::set_show(pool, user_id, remote.media_id, t).await?;
        }
        let tries = prior
            .filter(|k| k.meta.2 == remote.media_status)
            .map(|k| k.tries)
            .unwrap_or(0);
        let next = next_try(tries, p.outcome == Outcome::Upcoming);
        records::schedule_look(pool, user_id, remote.media_id, tries + 1, &next).await?;
        self.keep_local(remote, prior).await
    }

    /// `sync_local` for one entry, counted, with a failure that is not an
    /// outage kept to that entry.
    async fn keep_local(&mut self, remote: &Remote, prior: Option<&Known>) -> AppResult<()> {
        match sync_local(&self.acct, remote, prior).await {
            Ok((to_frog, to_anilist)) => {
                self.summary.local += 1;
                self.summary.to_frog += i64::from(to_frog);
                self.summary.to_anilist += i64::from(to_anilist);
                Ok(())
            }
            Err(e) if is_outage(&e) => Err(e),
            Err(e) => {
                tracing::warn!(media_id = remote.media_id, error = %e, "keeping an AniList entry failed");
                self.summary.failed += 1;
                Ok(())
            }
        }
    }

    /// A placed entry: moved if it has to be, then each side brought up to
    /// date with the other.
    async fn sync_placed(
        &mut self,
        remote: &Remote,
        prior: Option<&Known>,
        prior_place: Option<(Target, i64)>,
        Decision {
            target,
            verdict,
            candidates,
        }: Decision,
    ) -> AppResult<()> {
        let (state, user_id) = (self.acct.state, self.acct.user_id);
        let pool = &state.pool;
        let (kind, tmdb_id) = match target {
            Target::Movie(id) => ("movie", id),
            Target::Show { tmdb_id, .. } => ("tv", tmdb_id),
        };
        let title_id = match prior_place {
            Some((old, id)) if old == target => id,
            _ => {
                catalog::ensure(state, &ExternalId::tmdb(kind, tmdb_id))
                    .await?
                    .0
            }
        };

        // Moved: take back what the old place was given, then start this
        // entry afresh where it belongs.
        let moved = prior_place.filter(|(old, _)| *old != target);
        if let Some((old, old_title)) = moved {
            unplace(
                state,
                user_id,
                (remote.media_id, remote.episodes),
                old_title,
                old,
                Some((title_id, target)),
            )
            .await?;
            self.summary.repaired += 1;
        }
        // Kept as an AniList-only title until now: what this account has
        // there moves to the real title, which then starts afresh.
        let adopted = match adopt_local(state, user_id, remote, title_id, target).await {
            Ok(moved) => moved,
            Err(e) => {
                tracing::warn!(media_id = remote.media_id, error = %e, "moving an AniList-only title failed");
                self.summary.failed += 1;
                return Ok(());
            }
        };
        let fresh = moved.is_some() || adopted || prior.is_some_and(|k| k.state != "placed");
        let (snap_al, snap_frog) = if fresh {
            (None, None)
        } else {
            (
                prior.and_then(|k| k.al.clone()),
                prior.and_then(|k| k.frog.clone()),
            )
        };

        // The verdict of a kept placement stays as it was.
        if verdict.confidence >= 0 {
            save_entry(
                pool,
                user_id,
                remote,
                &verdict,
                Some((title_id, target)),
                Some(&candidates),
            )
            .await?;
        } else {
            records::refresh_kept(pool, user_id, remote).await?;
        }
        if kind == "tv" {
            self.touched.insert(title_id);
        }

        let a = &remote.view;
        let f = frog_view(
            pool,
            user_id,
            remote.media_id,
            title_id,
            target,
            remote.episodes,
        )
        .await?;

        // Removed from Frog after it was synced: leave it be from now on.
        if f.is_none() && snap_al.is_some() {
            save_snapshots(pool, user_id, remote.media_id, a, None, true).await?;
            return Ok(());
        }

        // Placed again by a newer matcher: AniList's progress is laid out
        // afresh where the entry now runs, which also fills in what an older,
        // shorter run had cut off.
        let replaced = verdict.confidence >= 0 && prior.is_some_and(|k| k.resolver < RESOLVER);
        let c = Changes::between(snap_al.as_ref(), snap_frog.as_ref(), a, f.as_ref());
        if let Some(mode) = c.mode(replaced) {
            let created = match apply(
                state,
                user_id,
                remote,
                title_id,
                target,
                mode,
                snap_al.as_ref(),
            )
            .await
            {
                Ok(created) => created,
                Err(e) if is_outage(&e) => return Err(e),
                Err(e) => {
                    // One entry that cannot be written does not stop the rest;
                    // Settings says which and why.
                    tracing::warn!(media_id = remote.media_id, error = %e, "applying an AniList entry failed");
                    records::apply_failed(pool, user_id, remote.media_id).await?;
                    return Ok(());
                }
            };
            if kind == "tv" {
                // New to the library, or changed on AniList: AniList decides.
                let firm = if created { Mode::Follow } else { mode };
                let slot = self.shows.entry(title_id).or_insert(firm);
                if firm == Mode::Follow {
                    *slot = Mode::Follow;
                }
            }
        }

        let f_now = frog_view(
            pool,
            user_id,
            remote.media_id,
            title_id,
            target,
            remote.episodes,
        )
        .await?;
        if f_now != f {
            self.summary.to_frog += 1;
        }

        // Frog has something AniList does not: send it.
        let mut final_al = a.clone();
        if let Some(now) = f_now.as_ref() {
            let fresh = c.first || replaced;
            if let Some(send) = reconcile::outgoing(fresh, &c, snap_frog.as_ref(), now, a) {
                final_al = push(self.acct.api, self.acct.token, remote.media_id, &send).await?;
                self.summary.to_anilist += 1;
            }
        }
        save_snapshots(
            pool,
            user_id,
            remote.media_id,
            &final_al,
            f_now.as_ref(),
            false,
        )
        .await
    }

    /// One status per show, once all of its seasons are in. Shows AniList
    /// changed take AniList's paused or dropped; every show is then held to
    /// what has come out, which is also what repairs a status an earlier
    /// sync got wrong. Then Frog's side of each of its entries is final.
    async fn settle_shows(&mut self, remotes: &[Remote]) -> AppResult<()> {
        let (state, user_id) = (self.acct.state, self.acct.user_id);
        let pool = &state.pool;
        for &title_id in &self.touched {
            let rows = records::placed_in(pool, user_id, title_id).await?;
            match self.shows.get(&title_id) {
                Some(&mode) => {
                    let entries: Vec<(i64, i64, String)> =
                        rows.iter().map(|r| (r.0, r.1, r.2.clone())).collect();
                    settle_show(state, user_id, title_id, &entries, mode).await?;
                }
                None => release::reconcile(pool, user_id, title_id).await?,
            }
            for (season, offset, _, media_id) in rows {
                let Some(remote) = remotes.iter().find(|r| r.media_id == media_id) else {
                    continue;
                };
                let target = Target::Show {
                    tmdb_id: 0,
                    season,
                    offset,
                };
                let f =
                    frog_view(pool, user_id, media_id, title_id, target, remote.episodes).await?;
                records::save_frog_snapshot(pool, user_id, media_id, f.as_ref()).await?;
            }
        }
        Ok(())
    }
}
