//! What the server's own jobs did: backups, updates, rollbacks and restores.
//!
//! `deploy/update.sh` and `deploy/backup.sh` run on the host, outside Frog, and
//! append one JSON line per outcome to `ops.jsonl` beside the database (see
//! `deploy/ops-log.sh`). Frog only reads it: the admin page shows the latest of
//! each, and administrators are told once about each new failure.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpsEvent {
    /// RFC 3339 in UTC, to the millisecond, so events sort as text.
    pub at: String,
    /// backup, update, rollback or restore.
    pub kind: String,
    pub ok: bool,
    pub reason: String,
}

/// The log beside the database, or nothing for an in-memory database.
pub fn log_path(database_url: &str) -> Option<PathBuf> {
    let db = crate::db::sqlite_path(database_url)?;
    Some(std::path::Path::new(&db).parent()?.join("ops.jsonl"))
}

/// Every event in the log, oldest first. No file means nothing has run yet
/// (or Frog runs without the deploy scripts); a line that does not parse is
/// skipped rather than hiding the rest.
pub fn read(database_url: &str) -> Vec<OpsEvent> {
    let Some(text) = log_path(database_url).and_then(|p| std::fs::read_to_string(p).ok()) else {
        return Vec::new();
    };
    let mut events: Vec<OpsEvent> = text
        .lines()
        .filter_map(|line| serde_json::from_str(line.trim()).ok())
        .collect();
    events.sort_by(|a, b| a.at.cmp(&b.at));
    events
}

/// The latest backup, the latest update, and the latest rollback or restore,
/// for the admin page.
pub fn summary(events: &[OpsEvent]) -> Value {
    let last = |kinds: &[&str]| {
        events
            .iter()
            .rev()
            .find(|e| kinds.contains(&e.kind.as_str()))
    };
    json!({
        "backup": last(&["backup"]),
        "update": last(&["update"]),
        "rollback": last(&["rollback", "restore"]),
    })
}

/// Failures to tell someone about, given the newest event they have already
/// been told about (`seen`), and the new mark to remember.
///
/// A failure that merely repeats the previous outcome of the same kind (an
/// update timer retrying the same broken release every 15 minutes) is not
/// news; the first one was. Somebody never told anything before starts from
/// now rather than being handed the whole history.
pub fn new_failures<'a>(
    events: &'a [OpsEvent],
    seen: Option<&str>,
) -> (Vec<&'a OpsEvent>, Option<String>) {
    let newest = events.last().map(|e| e.at.clone());
    let Some(seen) = seen else {
        return (Vec::new(), newest);
    };
    let mut out = Vec::new();
    for (i, e) in events.iter().enumerate() {
        if e.at.as_str() <= seen || e.ok {
            continue;
        }
        let before = events[..i].iter().rev().find(|p| p.kind == e.kind);
        let repeat = before.is_some_and(|p| !p.ok && p.reason == e.reason);
        if !repeat {
            out.push(e);
        }
    }
    (out, newest.or_else(|| Some(seen.to_string())))
}

/// The notification for one failure.
pub fn payload(e: &OpsEvent) -> Value {
    let title = match e.kind.as_str() {
        "backup" => "A backup failed",
        "update" => "An update failed",
        "rollback" => "Going back after an update failed",
        "restore" => "Frog is down after an update",
        _ => "Something on the server failed",
    };
    json!({
        "title": title,
        "body": e.reason,
        "url": "/admin#server",
        // One per kind: a newer one replaces it rather than stacking up.
        "tag": format!("frog-ops-{}", e.kind),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(at: &str, kind: &str, ok: bool, reason: &str) -> OpsEvent {
        OpsEvent {
            at: at.into(),
            kind: kind.into(),
            ok,
            reason: reason.into(),
        }
    }

    #[test]
    fn the_latest_of_each_is_summarised() {
        let events = [
            ev("2026-10-01T03:00:00.000Z", "backup", true, "a"),
            ev("2026-10-02T03:00:00.000Z", "backup", false, "b"),
            ev("2026-10-02T04:00:00.000Z", "update", false, "c"),
            ev("2026-10-02T04:01:00.000Z", "rollback", true, "d"),
        ];
        let s = summary(&events);
        assert_eq!(s["backup"]["reason"], "b");
        assert_eq!(s["update"]["reason"], "c");
        assert_eq!(s["rollback"]["reason"], "d");
        assert_eq!(summary(&[])["backup"], Value::Null);
    }
}
