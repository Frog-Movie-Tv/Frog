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
    /// The fingerprint of the deploy scripts that wrote it
    /// (`scripts_fingerprint`); none from scripts older than the fingerprint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scripts: Option<String>,
}

/// Where the image keeps its own release's deploy scripts.
const DEPLOY_DIR: &str = "/app/deploy";

/// The fingerprint `deploy/ops-log.sh` writes as `scripts`: the first twelve
/// hex characters of the SHA-256 of every `*.sh` and `systemd/*` file, in byte
/// order of their paths, each as its path, a newline and its contents without
/// carriage returns. None when there is no such folder (a build run from a
/// checkout).
pub fn scripts_fingerprint(dir: &std::path::Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let mut files: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".sh") && entry.path().is_file() {
            files.push(name);
        }
    }
    if let Ok(units) = std::fs::read_dir(dir.join("systemd")) {
        for entry in units.flatten() {
            if entry.path().is_file() {
                files.push(format!("systemd/{}", entry.file_name().to_string_lossy()));
            }
        }
    }
    if files.is_empty() {
        return None;
    }
    files.sort();
    let mut hash = Sha256::new();
    for f in &files {
        let text = std::fs::read(dir.join(f)).ok()?;
        hash.update(f.as_bytes());
        hash.update(b"\n");
        hash.update(
            text.iter()
                .copied()
                .filter(|b| *b != b'\r')
                .collect::<Vec<u8>>(),
        );
    }
    let hex: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    Some(hex[..12].to_string())
}

/// This release's own fingerprint, read once.
pub fn release_scripts() -> Option<&'static str> {
    static RELEASE: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    RELEASE
        .get_or_init(|| {
            let dir = std::env::var("FROG_DEPLOY_DIR").unwrap_or_else(|_| DEPLOY_DIR.into());
            scripts_fingerprint(std::path::Path::new(&dir))
        })
        .as_deref()
}

/// Whether the server's deploy scripts are this release's: true or false
/// from the newest outcome they recorded, none when either side is unknown
/// (no image fingerprint, or nothing recorded yet). Scripts from before the
/// fingerprint record none, and are older by that alone.
pub fn scripts_current(events: &[OpsEvent], release: Option<&str>) -> Option<bool> {
    let release = release?;
    let newest = events.last()?;
    Some(newest.scripts.as_deref() == Some(release))
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

    #[test]
    fn scripts_are_current_only_when_the_newest_outcome_names_this_release() {
        let mut old = ev("2026-11-15T03:40:00.000Z", "backup", true, "ok");
        assert_eq!(scripts_current(&[old.clone()], Some("abc")), Some(false));
        old.scripts = Some("abc".into());
        assert_eq!(scripts_current(&[old.clone()], Some("abc")), Some(true));
        assert_eq!(scripts_current(&[old.clone()], None), None);
        assert_eq!(scripts_current(&[], Some("abc")), None);
        // A line without the field still reads.
        let line = r#"{"at":"2026-11-15T03:40:00.000Z","kind":"backup","ok":true,"reason":"ok"}"#;
        let read: OpsEvent = serde_json::from_str(line).unwrap();
        assert_eq!(read.scripts, None);
    }

    /// `ops_scripts` from deploy/ops-log.sh, run over the files below.
    const SHELL_SAID: &str = "e5072fcc79c1";

    #[test]
    fn the_fingerprint_is_the_one_the_shell_script_writes() {
        // `bash -c '. deploy/ops-log.sh; ops_scripts'` over these files.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("systemd")).unwrap();
        std::fs::write(dir.path().join("b.sh"), "echo b\r\n").unwrap();
        std::fs::write(dir.path().join("a-x.sh"), "echo a\n").unwrap();
        std::fs::write(dir.path().join("notes.md"), "not counted").unwrap();
        std::fs::write(dir.path().join("systemd/u@.service"), "[Unit]\n").unwrap();
        // What the script printed for these same files.
        assert_eq!(scripts_fingerprint(dir.path()).as_deref(), Some(SHELL_SAID));
        assert_eq!(scripts_fingerprint(&dir.path().join("nowhere")), None);
    }

    fn ev(at: &str, kind: &str, ok: bool, reason: &str) -> OpsEvent {
        OpsEvent {
            at: at.into(),
            kind: kind.into(),
            ok,
            reason: reason.into(),
            scripts: None,
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
