//! Administration → Database: every table, read only, a page at a time.
//!
//! Nothing here writes. Each look opens its own connection, read only at the
//! file and with `query_only` on, so not even a mistake in this module could
//! change a row. Table and column names are never taken from the request: a
//! name the caller sends is only used to pick one that `sqlite_master` and
//! `pragma_table_info` already listed.
//!
//! Secrets never leave the server. `HIDDEN` is the one list of what is kept
//! back, and `tests/dbview.rs` fails when a new column whose name suggests a
//! secret is in neither it nor `PLAIN`. A hidden column cannot be searched or
//! sorted on either, since either would let someone guess it a letter at a
//! time. This view reads every account's rows on purpose, which is what
//! sitting behind `AdminAuth` is for.

use serde::Serialize;
use serde_json::{json, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqliteRow};
use sqlx::{ConnectOptions, Row, SqliteConnection, TypeInfo, ValueRef};
use std::str::FromStr;

use crate::error::{AppError, AppResult};

/// How a column reaches the browser when it is not shown as it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Hide {
    /// Only whether it holds anything.
    Secret,
    /// Only how many bytes it holds: pictures, cached answers, keys' blobs.
    Size,
}

/// Every column kept back, by table and column, with how.
pub const HIDDEN: &[(&str, &str, Hide)] = &[
    ("app_user", "password_hash", Hide::Secret),
    ("app_user", "totp_secret", Hide::Secret),
    ("app_user", "totp_pending", Hide::Secret),
    ("session", "token_hash", Hide::Secret),
    ("invite", "code_hash", Hide::Secret),
    ("password_reset", "code_hash", Hide::Secret),
    ("recovery_code", "code_hash", Hide::Secret),
    ("anilist_link", "token", Hide::Secret),
    // The OAuth state is what proves a returning AniList sign-in is ours.
    ("anilist_oauth", "state", Hide::Secret),
    ("push_subscription", "endpoint", Hide::Secret),
    ("push_subscription", "p256dh", Hide::Secret),
    ("push_subscription", "auth", Hide::Secret),
    ("push_vapid", "private_pkcs8", Hide::Secret),
    ("tmdb_cache", "body", Hide::Size),
    ("image_cache", "bytes", Hide::Size),
    ("user_image", "bytes", Hide::Size),
    ("_sqlx_migrations", "checksum", Hide::Size),
];

/// Columns whose names sound secret but are not, each with the reason.
pub const PLAIN: &[(&str, &str, &str)] = &[
    ("app_setting", "key", "the name of a setting, not a key"),
    (
        "tmdb_cache",
        "cache_key",
        "the TMDB address an answer was cached for",
    ),
    ("studio_match", "name_key", "a studio's folded name"),
];

/// Words in a column name that mean it may hold a secret.
pub const SECRET_WORDS: &[&str] = &["hash", "secret", "token", "key", "p256dh", "auth"];

/// How a column is shown: as it is, or kept back. Any BLOB column is shown
/// by size even when nobody listed it, since bytes are never worth reading.
pub fn hidden(table: &str, column: &str, declared: &str) -> Option<Hide> {
    HIDDEN
        .iter()
        .find(|(t, c, _)| *t == table && *c == column)
        .map(|(_, _, how)| *how)
        .or_else(|| declared.eq_ignore_ascii_case("BLOB").then_some(Hide::Size))
}

pub const PER_PAGE: u32 = 50;
pub const PER_PAGE_MAX: u32 = 200;

/// A connection that cannot write: read only at the file, and `query_only`
/// refuses any statement that would change it even so.
pub async fn reader(database_url: &str) -> AppResult<SqliteConnection> {
    let opts = SqliteConnectOptions::from_str(database_url)
        .map_err(|e| AppError::Other(e.into()))?
        .read_only(true)
        .create_if_missing(false)
        .pragma("query_only", "ON");
    Ok(opts.connect().await?)
}

#[derive(Serialize)]
pub struct TableInfo {
    pub name: String,
    pub rows: i64,
    /// Bytes on disk with its indexes, when SQLite can tell.
    pub bytes: Option<i64>,
}

/// Every table but SQLite's own, with its rows and size.
pub async fn tables(conn: &mut SqliteConnection) -> AppResult<Vec<TableInfo>> {
    let names = table_names(conn).await?;
    // `dbstat` is compiled into the bundled SQLite; when it is missing the
    // sizes are left out rather than the page failing.
    let sizes: Vec<(String, i64)> = sqlx::query_as(
        "SELECT m.tbl_name, SUM(d.pgsize) FROM dbstat d
           JOIN sqlite_master m ON m.name = d.name
          GROUP BY m.tbl_name",
    )
    .fetch_all(&mut *conn)
    .await
    .unwrap_or_default();
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let (rows,): (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {}", quoted(&name)))
            .fetch_one(&mut *conn)
            .await?;
        let bytes = sizes.iter().find(|(t, _)| *t == name).map(|(_, b)| *b);
        out.push(TableInfo { name, rows, bytes });
    }
    Ok(out)
}

async fn table_names(conn: &mut SqliteConnection) -> AppResult<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM sqlite_master
          WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'
          ORDER BY name",
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(|(n,)| n).collect())
}

#[derive(Serialize, Clone)]
pub struct ColumnInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub declared: String,
    pub primary_key: bool,
    pub hidden: Option<Hide>,
}

/// A table's columns as `pragma_table_info` lists them, or None for a name
/// that is not one of the database's tables.
pub async fn columns(
    conn: &mut SqliteConnection,
    table: &str,
) -> AppResult<Option<(String, Vec<ColumnInfo>)>> {
    let Some(name) = table_names(conn).await?.into_iter().find(|n| n == table) else {
        return Ok(None);
    };
    let rows: Vec<(String, String, i64)> =
        sqlx::query_as("SELECT name, type, pk FROM pragma_table_info(?) ORDER BY cid")
            .bind(&name)
            .fetch_all(&mut *conn)
            .await?;
    let cols = rows
        .into_iter()
        .map(|(column, declared, pk)| ColumnInfo {
            hidden: hidden(&name, &column, &declared),
            name: column,
            declared,
            primary_key: pk > 0,
        })
        .collect();
    Ok(Some((name, cols)))
}

pub struct Ask {
    pub page: u32,
    pub per_page: u32,
    pub sort: Option<String>,
    pub descending: bool,
    pub search: Option<String>,
}

#[derive(Serialize)]
pub struct Page {
    pub table: String,
    pub columns: Vec<ColumnInfo>,
    /// One array per row, in the columns' order. A secret is `true` when it
    /// holds something and `null` when not; a size is its byte count.
    pub rows: Vec<Vec<Value>>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
}

/// One page of a table. `None` when the table does not exist.
pub async fn page(conn: &mut SqliteConnection, table: &str, ask: Ask) -> AppResult<Option<Page>> {
    let Some((table, cols)) = columns(conn, table).await? else {
        return Ok(None);
    };
    let per_page = ask.per_page.clamp(1, PER_PAGE_MAX);
    let page = ask.page.max(1);

    let order = match ask.sort.as_deref().filter(|s| !s.is_empty()) {
        None => "rowid".to_string(),
        Some(wanted) => {
            let col = cols
                .iter()
                .find(|c| c.name == wanted)
                .ok_or_else(|| AppError::BadRequest("No such column.".into()))?;
            if col.hidden.is_some() {
                return Err(AppError::BadRequest("That column cannot be sorted.".into()));
            }
            quoted(&col.name)
        }
    };
    let dir = if ask.descending { "DESC" } else { "ASC" };
    // A table without rowid has none to fall back on.
    let order = if order == "rowid" && !has_rowid(conn, &table).await? {
        "1".to_string()
    } else {
        order
    };

    let term = ask
        .search
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| {
            format!(
                "%{}%",
                t.replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            )
        });
    let searchable: Vec<&ColumnInfo> = cols.iter().filter(|c| c.hidden.is_none()).collect();
    let filter = match &term {
        Some(_) if !searchable.is_empty() => format!(
            "WHERE {}",
            searchable
                .iter()
                .map(|c| format!("CAST({} AS TEXT) LIKE ?1 ESCAPE '\\'", quoted(&c.name)))
                .collect::<Vec<_>>()
                .join(" OR ")
        ),
        Some(_) => "WHERE 0".to_string(),
        None => String::new(),
    };

    let select = cols
        .iter()
        .map(|c| {
            let q = quoted(&c.name);
            match c.hidden {
                None => q,
                Some(Hide::Secret) => {
                    format!("CASE WHEN {q} IS NULL OR {q} = '' THEN NULL ELSE 1 END")
                }
                Some(Hide::Size) => format!("length(CAST({q} AS BLOB))"),
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let from = quoted(&table);

    let counting = format!("SELECT COUNT(*) FROM {from} {filter}");
    let mut count = sqlx::query_as::<_, (i64,)>(&counting);
    if let Some(t) = &term {
        count = count.bind(t);
    }
    let (total,) = count.fetch_one(&mut *conn).await?;

    let sql = format!(
        "SELECT {select} FROM {from} {filter} ORDER BY {order} {dir} LIMIT {per_page} OFFSET {}",
        (page as i64 - 1) * per_page as i64
    );
    let mut rows = sqlx::query(&sql);
    if let Some(t) = &term {
        rows = rows.bind(t);
    }
    let rows = rows.fetch_all(&mut *conn).await?;
    let rows = rows
        .iter()
        .map(|row| {
            cols.iter()
                .enumerate()
                .map(|(i, c)| match c.hidden {
                    Some(Hide::Secret) => match cell(row, i) {
                        Value::Null => Value::Null,
                        _ => json!(true),
                    },
                    _ => cell(row, i),
                })
                .collect()
        })
        .collect();
    Ok(Some(Page {
        table,
        columns: cols,
        rows,
        total,
        page,
        per_page,
    }))
}

async fn has_rowid(conn: &mut SqliteConnection, table: &str) -> AppResult<bool> {
    let (sql,): (String,) = sqlx::query_as("SELECT sql FROM sqlite_master WHERE name = ?")
        .bind(table)
        .fetch_one(&mut *conn)
        .await?;
    Ok(!sql.to_ascii_uppercase().contains("WITHOUT ROWID"))
}

/// One value as SQLite holds it. Bytes that reach here (a BLOB in a column
/// declared otherwise) go out as their size alone.
fn cell(row: &SqliteRow, i: usize) -> Value {
    let Ok(raw) = row.try_get_raw(i) else {
        return Value::Null;
    };
    if raw.is_null() {
        return Value::Null;
    }
    match raw.type_info().name() {
        "INTEGER" | "BOOLEAN" => row
            .try_get::<i64, _>(i)
            .map(Value::from)
            .unwrap_or(Value::Null),
        "REAL" => row
            .try_get::<f64, _>(i)
            .map(Value::from)
            .unwrap_or(Value::Null),
        "BLOB" => row
            .try_get::<Vec<u8>, _>(i)
            .map(|b| json!({ "bytes": b.len() }))
            .unwrap_or(Value::Null),
        _ => row
            .try_get::<String, _>(i)
            .map(Value::from)
            .unwrap_or(Value::Null),
    }
}

/// A name SQLite listed, quoted as an identifier.
fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listed_secrets_and_any_blob_are_kept_back() {
        assert_eq!(
            hidden("app_user", "password_hash", "TEXT"),
            Some(Hide::Secret)
        );
        assert_eq!(hidden("image_cache", "bytes", "BLOB"), Some(Hide::Size));
        assert_eq!(hidden("anything_new", "picture", "blob"), Some(Hide::Size));
        assert_eq!(hidden("app_user", "username", "TEXT"), None);
    }

    #[test]
    fn a_name_is_quoted_as_an_identifier() {
        assert_eq!(quoted("title"), "\"title\"");
        assert_eq!(quoted("a\"b"), "\"a\"\"b\"");
    }
}
