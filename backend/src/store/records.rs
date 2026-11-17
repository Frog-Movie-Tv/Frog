//! A library entry whole, as rows: what removing a title takes, what undo
//! and backups carry, and putting it back exactly.

use super::*;

/// A library entry whole, as a row: what it takes to put one back exactly.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EntryRecord {
    pub title_id: i64,
    pub status: String,
    pub rating: Option<f64>,
    pub favourite: bool,
    pub notes: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub rewatch_count: i64,
    pub added_at: String,
    pub updated_at: String,
    pub notify: bool,
    pub progress: Option<i64>,
    /// Its watches, which leave with it and come back with undo.
    #[sqlx(skip)]
    #[serde(default)]
    pub watches: Vec<WatchRecord>,
    /// Its episodes' ratings and notes, likewise.
    #[sqlx(skip)]
    #[serde(default)]
    pub episode_notes: Vec<EpisodeNoteRecord>,
    /// A manga's chapters as they were read, likewise.
    #[sqlx(skip)]
    #[serde(default)]
    pub reads: Vec<ReadRecord>,
}

/// One chapter of a manga read on a day (`chapter_read`).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ReadRecord {
    pub chapter: i64,
    pub read_at: String,
    pub source: String,
}

/// Where a read can have come from.
const READ_SOURCES: [&str; 3] = ["manual", "anilist", "import"];

/// The most chapters one change records: more than any manga has.
const MOST_CHAPTERS: i64 = 5000;

/// One watch, by episode number rather than Frog's row ids, so that what comes
/// back from the browser can only ever name an episode of the same title.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WatchRecord {
    /// None for a film's own watch.
    pub season: Option<i64>,
    pub episode: Option<i64>,
    pub watched_at: String,
    pub is_rewatch: bool,
    pub source: String,
    pub undated: bool,
    /// Who it was watched with, by account (`watch_with`), so undo keeps them.
    #[sqlx(skip)]
    #[serde(default)]
    pub with: Vec<i64>,
    /// When and how it reached Frog, so undo puts back a watch that still
    /// counts for the badges it counted for (`crate::awards::eligible`).
    #[serde(default)]
    pub logged_at: Option<String>,
    #[serde(default)]
    pub logged_how: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EpisodeNoteRecord {
    pub season: i64,
    pub episode: i64,
    pub rating: Option<f64>,
    pub notes: Option<String>,
    pub updated_at: String,
}

/// Where a watch can have come from.
const SOURCES: [&str; 5] = ["manual", "import", "anilist", "netflix", "trakt"];

/// This account's entries for these titles, whole.
pub async fn entry_records(
    pool: &SqlitePool,
    user_id: i64,
    title_ids: &[i64],
) -> AppResult<Vec<EntryRecord>> {
    let mut out = Vec::with_capacity(title_ids.len());
    for ids in title_ids.chunks(IDS_AT_ONCE) {
        let mut q: QueryBuilder<Sqlite> = QueryBuilder::new(
            "SELECT title_id, status, rating, favourite, notes, started_at, finished_at,
                    rewatch_count, added_at, updated_at, notify, progress
               FROM library_entry WHERE user_id = ",
        );
        q.push_bind(user_id).push(" AND title_id IN (");
        let mut list = q.separated(", ");
        for id in ids {
            list.push_bind(*id);
        }
        q.push(")");
        out.extend(q.build_query_as::<EntryRecord>().fetch_all(pool).await?);
    }
    Ok(out)
}

/// A title's watches and episode notes in this account, to keep for undo.
pub async fn held_watches(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
) -> AppResult<(Vec<WatchRecord>, Vec<EpisodeNoteRecord>)> {
    let mut watches = sqlx::query_as::<_, WatchRecord>(
        "SELECT e.season_number AS season, e.episode_number AS episode, w.watched_at,
                w.is_rewatch, w.source, w.undated, w.logged_at, w.logged_how
           FROM watch_event w
           LEFT JOIN episode e ON e.id = w.episode_id
          WHERE w.user_id = ? AND w.title_id = ?",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_all(pool)
    .await?;
    // Who each was with, matched the way a watch is put back: by episode and moment.
    let with: Vec<(Option<i64>, Option<i64>, String, i64)> = sqlx::query_as(
        "SELECT e.season_number, e.episode_number, w.watched_at, x.with_user_id
           FROM watch_with x
           JOIN watch_event w ON w.id = x.watch_event_id
           LEFT JOIN episode e ON e.id = w.episode_id
          WHERE x.user_id = ?1 AND w.user_id = ?1 AND w.title_id = ?2",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_all(pool)
    .await?;
    for (season, episode, at, person) in with {
        if let Some(w) = watches
            .iter_mut()
            .find(|w| w.season == season && w.episode == episode && w.watched_at == at)
        {
            if !w.with.contains(&person) {
                w.with.push(person);
            }
        }
    }
    let notes = sqlx::query_as::<_, EpisodeNoteRecord>(
        "SELECT e.season_number AS season, e.episode_number AS episode,
                CAST(NULL AS REAL) AS rating, s.notes, s.updated_at
           FROM episode_state s
           JOIN episode e ON e.id = s.episode_id
          WHERE s.user_id = ? AND e.title_id = ?",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_all(pool)
    .await?;
    Ok((watches, notes))
}

/// A manga's reads in this account, to keep for undo and backups.
pub async fn held_reads(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
) -> AppResult<Vec<ReadRecord>> {
    Ok(sqlx::query_as(
        "SELECT chapter, read_at, source FROM chapter_read
          WHERE user_id = ? AND title_id = ? ORDER BY chapter",
    )
    .bind(user_id)
    .bind(title_id)
    .fetch_all(pool)
    .await?)
}

/// Chapters `after + 1` to `upto` of a manga, read at `at`. The entry must be
/// in the library; a chapter already read at that moment is not read twice.
pub async fn record_reads(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    (after, upto): (i64, i64),
    at: &str,
    source: &str,
) -> AppResult<()> {
    let from = after.max(0) + 1;
    let upto = upto.min(after.max(0) + MOST_CHAPTERS);
    if upto < from {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    for chapter in from..=upto {
        sqlx::query(
            "INSERT INTO chapter_read (user_id, title_id, chapter, read_at, source)
             SELECT ?1, ?2, ?3, ?4, ?5
              WHERE NOT EXISTS (SELECT 1 FROM chapter_read
                                 WHERE user_id = ?1 AND title_id = ?2 AND chapter = ?3
                                   AND read_at = ?4)",
        )
        .bind(user_id)
        .bind(title_id)
        .bind(chapter)
        .bind(at)
        .bind(source)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Take back the reads of chapters past `upto`: from one source, or from
/// every source when it is None.
pub async fn take_back_reads(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    upto: i64,
    source: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        "DELETE FROM chapter_read
          WHERE user_id = ? AND title_id = ? AND chapter > ? AND (? IS NULL OR source = ?)",
    )
    .bind(user_id)
    .bind(title_id)
    .bind(upto)
    .bind(source)
    .bind(source)
    .execute(pool)
    .await?;
    Ok(())
}

/// Remove these titles from this account's library, all or none, and with
/// them what was watched of them: a title taken out is no longer watched, in
/// its episodes, the diary or the stats. A manga's reads go with its entry,
/// on the foreign key.
pub async fn remove_entries(pool: &SqlitePool, user_id: i64, title_ids: &[i64]) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    for ids in title_ids.chunks(IDS_AT_ONCE) {
        // (before the account, between it and the ids, after the ids)
        for (head, middle, tail) in [
            (
                "DELETE FROM library_entry WHERE user_id = ",
                " AND title_id IN (",
                ")",
            ),
            (
                "DELETE FROM watch_event WHERE user_id = ",
                " AND title_id IN (",
                ")",
            ),
            (
                "DELETE FROM episode_state WHERE user_id = ",
                " AND episode_id IN (SELECT id FROM episode WHERE title_id IN (",
                "))",
            ),
        ] {
            let mut q: QueryBuilder<Sqlite> = QueryBuilder::new(head);
            q.push_bind(user_id).push(middle);
            let mut list = q.separated(", ");
            for id in ids {
                list.push_bind(*id);
            }
            q.push(tail);
            q.build().execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Put entries back exactly as `entry_records` read them, all or none. They
/// come back from the browser, so each is checked, it lands in this account
/// whatever it says, and a title that has gone from the catalog is passed over.
/// With `replace`, the watches and reads a title has now go first, so what
/// was added since (by marking it watched) is taken back.
pub async fn restore_entries(
    pool: &SqlitePool,
    user_id: i64,
    entries: &[EntryRecord],
    replace: bool,
) -> AppResult<u64> {
    for e in entries {
        if !STATUSES.contains(&e.status.as_str()) {
            return Err(AppError::BadRequest(format!("unknown status {}", e.status)));
        }
        if e.rating.is_some_and(|r| !(0.0..=100.0).contains(&r)) {
            return Err(AppError::BadRequest(
                "rating must be between 0 and 100".into(),
            ));
        }
    }
    let mut restored = 0;
    let mut tx = pool.begin().await?;
    for e in entries {
        if replace {
            for sql in [
                "DELETE FROM watch_event WHERE user_id = ? AND title_id = ?",
                "DELETE FROM chapter_read WHERE user_id = ? AND title_id = ?",
            ] {
                sqlx::query(sql)
                    .bind(user_id)
                    .bind(e.title_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        restored += sqlx::query(
            "INSERT INTO library_entry (user_id, title_id, status, rating, favourite, notes,
                                        started_at, finished_at, rewatch_count, added_at,
                                        updated_at, notify, progress)
             SELECT ?, id, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ? FROM title WHERE id = ?
             ON CONFLICT (user_id, title_id) DO UPDATE SET
                status = excluded.status, rating = excluded.rating,
                favourite = excluded.favourite, notes = excluded.notes,
                started_at = excluded.started_at, finished_at = excluded.finished_at,
                rewatch_count = excluded.rewatch_count, added_at = excluded.added_at,
                updated_at = excluded.updated_at, notify = excluded.notify,
                progress = excluded.progress",
        )
        .bind(user_id)
        .bind(&e.status)
        .bind(e.rating)
        .bind(e.favourite)
        .bind(&e.notes)
        .bind(&e.started_at)
        .bind(&e.finished_at)
        .bind(e.rewatch_count)
        .bind(&e.added_at)
        .bind(&e.updated_at)
        .bind(e.notify)
        .bind(e.progress)
        .bind(e.title_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        // Its watches, each matched to its episode by number; a watch put
        // back twice is still one watch.
        for w in &e.watches {
            let source = if SOURCES.contains(&w.source.as_str()) {
                w.source.as_str()
            } else {
                "manual"
            };
            let (logged_at, logged_how) =
                crate::awards::eligible::carried(w.logged_at.as_deref(), w.logged_how.as_deref());
            sqlx::query(
                "INSERT INTO watch_event (user_id, title_id, episode_id, watched_at, is_rewatch,
                                          source, undated, logged_at, logged_how)
                 SELECT ?1, t.id,
                        (SELECT e.id FROM episode e
                          WHERE e.title_id = t.id AND e.season_number = ?3
                            AND e.episode_number = ?4),
                        ?5, ?6, ?7, ?8, ?9, ?10
                   FROM title t
                  WHERE t.id = ?2
                    AND (?3 IS NULL OR EXISTS (SELECT 1 FROM episode e
                          WHERE e.title_id = t.id AND e.season_number = ?3
                            AND e.episode_number = ?4))
                    AND NOT EXISTS (SELECT 1 FROM watch_event x
                          WHERE x.user_id = ?1 AND x.title_id = t.id AND x.watched_at = ?5
                            AND x.episode_id IS (SELECT e.id FROM episode e
                                  WHERE e.title_id = t.id AND e.season_number = ?3
                                    AND e.episode_number = ?4))",
            )
            .bind(user_id)
            .bind(e.title_id)
            .bind(w.season)
            .bind(w.episode)
            .bind(&w.watched_at)
            .bind(w.is_rewatch)
            .bind(source)
            .bind(w.undated)
            .bind(logged_at)
            .bind(logged_how)
            .execute(&mut *tx)
            .await?;
            for person in &w.with {
                sqlx::query(
                    "INSERT OR IGNORE INTO watch_with (watch_event_id, user_id, with_user_id,
                                                       created_at)
                     SELECT x.id, ?1, u.id, ?6
                       FROM watch_event x JOIN app_user u ON u.id = ?5 AND u.disabled = 0
                      WHERE x.user_id = ?1 AND x.title_id = ?2 AND x.watched_at = ?4
                        AND x.episode_id IS (SELECT e.id FROM episode e
                              WHERE e.title_id = ?2 AND e.season_number = ?3
                                AND e.episode_number = ?7)",
                )
                .bind(user_id)
                .bind(e.title_id)
                .bind(w.season)
                .bind(&w.watched_at)
                .bind(person)
                .bind(Utc::now().stamp())
                .bind(w.episode)
                .execute(&mut *tx)
                .await?;
            }
        }
        for r in e.reads.iter().filter(|r| r.chapter > 0) {
            let source = if READ_SOURCES.contains(&r.source.as_str()) {
                r.source.as_str()
            } else {
                "manual"
            };
            sqlx::query(
                "INSERT INTO chapter_read (user_id, title_id, chapter, read_at, source)
                 SELECT ?1, ?2, ?3, ?4, ?5
                  WHERE EXISTS (SELECT 1 FROM library_entry WHERE user_id = ?1 AND title_id = ?2)
                    AND NOT EXISTS (SELECT 1 FROM chapter_read
                                     WHERE user_id = ?1 AND title_id = ?2 AND chapter = ?3
                                       AND read_at = ?4)",
            )
            .bind(user_id)
            .bind(e.title_id)
            .bind(r.chapter)
            .bind(&r.read_at)
            .bind(source)
            .execute(&mut *tx)
            .await?;
        }
        for n in &e.episode_notes {
            sqlx::query(
                "INSERT INTO episode_state (user_id, episode_id, notes, updated_at)
                 SELECT ?, e.id, ?, ? FROM episode e
                  WHERE e.title_id = ? AND e.season_number = ? AND e.episode_number = ?
                 ON CONFLICT (user_id, episode_id) DO UPDATE SET
                    notes = excluded.notes, updated_at = excluded.updated_at",
            )
            .bind(user_id)
            .bind(&n.notes)
            .bind(&n.updated_at)
            .bind(e.title_id)
            .bind(n.season)
            .bind(n.episode)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(restored)
}
