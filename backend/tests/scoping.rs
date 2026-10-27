//! Every query that touches personal data must name whose it is.
//!
//! Frog was built for one person, so for a long time no query needed a
//! `user_id` and the habit of leaving it out is the easy mistake to make. The
//! isolation tests in `api.rs` prove the routes that exist today keep accounts
//! apart; this one reads the source and fails the moment a *new* query forgets,
//! which is cheaper than finding out from a bug report.
//!
//! It is deliberately dumb: it looks at SQL string literals and checks they
//! mention `user_id`. Anything genuinely shared is listed in `ALLOWED` with
//! the reason, so skipping the rule is a visible decision rather than an
//! oversight.

use std::fs;
use std::path::{Path, PathBuf};

/// Tables that gained a `user_id` in migration 0008, and personal tables
/// added since.
const PERSONAL: [&str; 13] = [
    "user_image",
    "anilist_link",
    "anilist_entry",
    "anilist_oauth",
    "library_entry",
    "episode_state",
    "watch_event",
    "app_setting",
    "shelf",
    "shelf_item",
    "push_subscription",
    "push_sent",
    "recovery_code",
];

/// Fragments that are pasted into a statement and carry the account
/// themselves, so the statement around them does not repeat it.
const SCOPED_FRAGMENTS: [&str; 6] = [
    "{filter}",
    "{rated}",
    "{mine}",
    "{scope}",
    "{watched_eps}",
    "{film_seen}",
];

/// Statements that are global on purpose, each with its reason.
const ALLOWED: [(&str, &str); 4] = [
    (
        "DELETE FROM session WHERE expires_at",
        "housekeeping: expired sessions of every account go at once",
    ),
    (
        "FROM title t WHERE t.people_synced_at IS NULL",
        "decides which shared titles need credits; reads library_entry only to \
         see whether anybody tracks them, and writes nothing personal",
    ),
    (
        "SELECT t.id, t.tmdb_id FROM library_entry l JOIN title t",
        "refreshes shared episode data for shows somebody tracks; writes only \
         title and episode",
    ),
    (
        "SELECT DISTINCT user_id FROM push_subscription",
        "the worker asks which accounts to notify before scoping to each",
    ),
];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read source directory") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Pull out double-quoted literals, spanning lines. None of this codebase's
/// SQL contains an escaped quote, which keeps this honest and simple.
fn literals(source: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let bytes: Vec<char> = source.chars().collect();
    let mut i = 0;
    let mut line = 1;
    while i < bytes.len() {
        match bytes[i] {
            '\n' => line += 1,
            '"' => {
                let start_line = line;
                let mut text = String::new();
                i += 1;
                while i < bytes.len() && bytes[i] != '"' {
                    if bytes[i] == '\\' {
                        i += 1;
                        if i >= bytes.len() {
                            break;
                        }
                    }
                    if bytes[i] == '\n' {
                        line += 1;
                    }
                    text.push(bytes[i]);
                    i += 1;
                }
                found.push((start_line, text));
            }
            _ => {}
        }
        i += 1;
    }
    found
}

/// The personal tables a statement reads or writes.
fn personal_tables(sql: &str) -> Vec<&'static str> {
    let flat = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = flat.to_lowercase();
    let mut hit = Vec::new();
    for table in PERSONAL {
        for keyword in ["from", "join", "into", "update", "delete from"] {
            if lower.contains(&format!("{keyword} {table} ")) {
                hit.push(table);
                break;
            }
            // End of statement, or followed by a bracketed column list.
            if lower.ends_with(&format!("{keyword} {table}"))
                || lower.contains(&format!("{keyword} {table}("))
            {
                hit.push(table);
                break;
            }
        }
    }
    hit
}

#[test]
fn every_query_touching_personal_data_names_an_owner() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    assert!(!files.is_empty(), "found no sources to check");

    let mut checked = 0;
    let mut problems = Vec::new();

    for file in &files {
        let source = fs::read_to_string(file).expect("read source");
        for (line, sql) in literals(&source) {
            let tables = personal_tables(&sql);
            if tables.is_empty() {
                continue;
            }
            let flat = sql.split_whitespace().collect::<Vec<_>>().join(" ");
            if ALLOWED.iter().any(|(pattern, _)| flat.contains(pattern)) {
                continue;
            }
            checked += 1;

            // shelf_item is only ever reached through a shelf whose ownership
            // has already been checked, so naming the shelf is enough.
            let only_items = tables == ["shelf_item"];
            let scoped = sql.contains("user_id")
                || SCOPED_FRAGMENTS.iter().any(|f| sql.contains(f))
                || (only_items && sql.contains("shelf_id"));

            if !scoped {
                let name = file.file_name().unwrap().to_string_lossy();
                let shown: String = flat.chars().take(110).collect();
                problems.push(format!("  {name}:{line}  {shown}"));
            }
        }
    }

    assert!(
        checked > 40,
        "only {checked} statements were examined; the reader is probably broken"
    );
    assert!(
        problems.is_empty(),
        "{} {} personal data without naming an owner.\n\
         Add `user_id` to them, or, if the statement really is shared, list it \
         in ALLOWED in this test with the reason:\n{}",
        problems.len(),
        if problems.len() == 1 {
            "query touches"
        } else {
            "queries touch"
        },
        problems.join("\n")
    );
}
