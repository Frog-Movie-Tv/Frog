//! Badges, and holding them.
//!
//! A badge is shared (`badge`), like a title; holding one is personal
//! (`badge_award`), and the holder decides whether it shows and where. Most
//! are made by hand in Administration and given one at a time; their pictures
//! are finished in the browser, round and lossless, and only checked here: a
//! PNG, square, the two sizes exactly. The rest Frog gives on its own
//! (`crate::awards`): those carry their catalogue key in `rule`, and their
//! pictures are drawn from the catalogue's art until an administrator redraws
//! one, so their rows hold none.

use crate::clock::Stamp;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::awards::custom::Condition;
use crate::error::{AppError, AppResult};

/// The picture opened large, and the one drawn everywhere else.
pub const LARGE_PIXELS: u32 = 1024;
pub const SMALL_PIXELS: u32 = 128;

/// A round 1024 px PNG of a detailed picture stays well under this.
pub const LARGE_MAX_BYTES: usize = 4 * 1024 * 1024;
pub const SMALL_MAX_BYTES: usize = 256 * 1024;
/// The upload as it was, already shrunk past 4 MB by the editor.
pub const ORIGINAL_MAX_BYTES: usize = 8 * 1024 * 1024;
/// The editor's own settings: a crop, a few sliders, a colour or two, and
/// the brush strokes painted by hand.
const SETTINGS_MAX_BYTES: usize = 128 * 1024;

pub const NAME_MAX: usize = 32;
pub const DESCRIPTION_MAX: usize = 140;

// --- checks, on plain values --------------------------------------------

/// A badge's name: trimmed, one line, short enough to sit beside a name.
pub fn check_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("Give the badge a name.".into()));
    }
    if name.chars().count() > NAME_MAX {
        return Err(AppError::BadRequest(format!(
            "Keep a badge's name under {NAME_MAX} characters."
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(AppError::BadRequest(
            "A badge's name fits on one line.".into(),
        ));
    }
    Ok(name.to_string())
}

/// One line about it, or nothing.
pub fn check_description(text: Option<&str>) -> AppResult<Option<String>> {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if text.chars().count() > DESCRIPTION_MAX {
        return Err(AppError::BadRequest(format!(
            "Keep the description under {DESCRIPTION_MAX} characters."
        )));
    }
    if text.chars().any(char::is_control) {
        return Err(AppError::BadRequest(
            "A badge's description fits on one line.".into(),
        ));
    }
    Ok(Some(text.to_string()))
}

/// Width and height from a PNG's header, when the bytes are one.
pub fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") || bytes.len() < 24 || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((width, height))
}

/// A finished badge picture: a PNG, square, exactly `side` pixels, under
/// `max` bytes, and one that really decodes.
pub fn check_picture(bytes: &[u8], side: u32, max: usize) -> AppResult<()> {
    let what = if side == LARGE_PIXELS {
        "large"
    } else {
        "small"
    };
    if bytes.len() > max {
        return Err(AppError::BadRequest(format!(
            "The {what} picture is too large."
        )));
    }
    let Some((width, height)) = png_size(bytes) else {
        return Err(AppError::BadRequest(format!(
            "The {what} picture must be a PNG."
        )));
    };
    if width != height {
        return Err(AppError::BadRequest(format!(
            "The {what} picture must be square."
        )));
    }
    if width != side {
        return Err(AppError::BadRequest(format!(
            "The {what} picture must be {side} pixels across."
        )));
    }
    image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .map_err(|_| AppError::BadRequest(format!("The {what} picture could not be read.")))?;
    Ok(())
}

/// What was uploaded, kept so the editor can reopen it: pixels only.
pub fn check_original(bytes: &[u8]) -> AppResult<&'static str> {
    if bytes.len() > ORIGINAL_MAX_BYTES {
        return Err(AppError::BadRequest(
            "The original picture is too large.".into(),
        ));
    }
    match crate::images::sniff(bytes) {
        Some(mime @ ("image/png" | "image/jpeg" | "image/webp")) => Ok(mime),
        _ => Err(AppError::BadRequest(
            "Keep the original as a PNG, JPEG or WebP picture.".into(),
        )),
    }
}

/// The editor's settings: any JSON object, small. Frog never reads inside it.
pub fn check_settings(json: &str) -> AppResult<String> {
    if json.len() > SETTINGS_MAX_BYTES {
        return Err(AppError::BadRequest(
            "Those editor settings are too large.".into(),
        ));
    }
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(v) if v.is_object() => Ok(v.to_string()),
        _ => Err(AppError::BadRequest(
            "Those editor settings cannot be read.".into(),
        )),
    }
}

// --- the shared badges ----------------------------------------------------

/// Bytes travel as base64 in JSON: undo sends a removed badge back whole.
mod b64 {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(d)?;
        base64::engine::general_purpose::STANDARD
            .decode(text.as_bytes())
            .map_err(serde::de::Error::custom)
    }
}

/// The three pictures of a badge, checked.
#[derive(Serialize, Deserialize)]
pub struct Pictures {
    #[serde(with = "b64")]
    pub large: Vec<u8>,
    #[serde(with = "b64")]
    pub small: Vec<u8>,
    #[serde(with = "b64")]
    pub original: Vec<u8>,
    pub original_mime: String,
}

impl Pictures {
    pub fn check(large: Vec<u8>, small: Vec<u8>, original: Vec<u8>) -> AppResult<Self> {
        check_picture(&large, LARGE_PIXELS, LARGE_MAX_BYTES)?;
        check_picture(&small, SMALL_PIXELS, SMALL_MAX_BYTES)?;
        let original_mime = check_original(&original)?.to_string();
        Ok(Self {
            large,
            small,
            original,
            original_mime,
        })
    }
}

/// A badge as Administration lists it.
#[derive(Serialize)]
pub struct Badge {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Everyone holding it, by account id.
    pub holders: Vec<i64>,
    /// Set for one Frog gives on its own: its catalogue key.
    pub rule: Option<String>,
    /// For one made here: what earns it on its own, when anything does.
    pub condition: Option<Condition>,
}

/// (id, name, description, created_at, updated_at, rule, condition)
type BadgeRow = (
    i64,
    String,
    Option<String>,
    String,
    String,
    Option<String>,
    Option<String>,
);

/// Every badge, by name, with who holds each.
pub async fn all(pool: &SqlitePool) -> AppResult<Vec<Badge>> {
    let rows: Vec<BadgeRow> = sqlx::query_as(
        "SELECT id, name, description, created_at, updated_at, rule, condition FROM badge
          ORDER BY rule IS NOT NULL, name COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?;
    let held: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT a.badge_id, a.user_id FROM badge_award a ORDER BY a.awarded_at, a.user_id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, name, description, created_at, updated_at, rule, condition)| Badge {
                holders: held
                    .iter()
                    .filter(|(b, _)| *b == id)
                    .map(|(_, u)| *u)
                    .collect(),
                id,
                name,
                description,
                created_at,
                updated_at,
                rule,
                condition: condition.as_deref().and_then(Condition::parse),
            },
        )
        .collect())
}

/// One badge, with the editor's settings, for editing it.
pub async fn one(pool: &SqlitePool, id: i64) -> AppResult<Option<(Badge, String)>> {
    type OneRow = (
        i64,
        String,
        Option<String>,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
    );
    let row: Option<OneRow> = sqlx::query_as(
        "SELECT id, name, description, created_at, updated_at, settings_json, rule, condition
           FROM badge WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let Some((id, name, description, created_at, updated_at, settings, rule, condition)) = row
    else {
        return Ok(None);
    };
    let holders: Vec<(i64,)> =
        sqlx::query_as("SELECT a.user_id FROM badge_award a WHERE a.badge_id = ?")
            .bind(id)
            .fetch_all(pool)
            .await?;
    Ok(Some((
        Badge {
            id,
            name,
            description,
            created_at,
            updated_at,
            holders: holders.into_iter().map(|(u,)| u).collect(),
            rule,
            condition: condition.as_deref().and_then(Condition::parse),
        },
        settings,
    )))
}

async fn name_taken(pool: &SqlitePool, name: &str, other_than: Option<i64>) -> AppResult<bool> {
    let (taken,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM badge WHERE name = ? COLLATE NOCASE AND id IS NOT ?)",
    )
    .bind(name)
    .bind(other_than)
    .fetch_one(pool)
    .await?;
    Ok(taken)
}

fn taken() -> AppError {
    AppError::Conflict("There is already a badge with that name.".into())
}

pub async fn create(
    pool: &SqlitePool,
    name: &str,
    description: Option<&str>,
    settings: &str,
    pictures: &Pictures,
) -> AppResult<i64> {
    if name_taken(pool, name, None).await? {
        return Err(taken());
    }
    let now = Utc::now().stamp();
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO badge (name, description, large, small, original, original_mime,
                            settings_json, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(name)
    .bind(description)
    .bind(&pictures.large)
    .bind(&pictures.small)
    .bind(&pictures.original)
    .bind(&pictures.original_mime)
    .bind(settings)
    .bind(&now)
    .bind(&now)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// A new name, description or pictures. Says whether the badge exists.
pub async fn update(
    pool: &SqlitePool,
    id: i64,
    name: &str,
    description: Option<&str>,
    settings: Option<&str>,
    pictures: Option<&Pictures>,
) -> AppResult<bool> {
    if name_taken(pool, name, Some(id)).await? {
        return Err(taken());
    }
    let now = Utc::now().stamp();
    // An automatic badge renamed or reworded keeps its words from then on.
    let found = sqlx::query(
        "UPDATE badge
            SET rule_kept = CASE WHEN rule IS NOT NULL
                                      AND (name IS NOT ?1 OR description IS NOT ?2)
                                 THEN 1 ELSE rule_kept END,
                name = ?1, description = ?2,
                settings_json = COALESCE(?3, settings_json), updated_at = ?4
          WHERE id = ?5",
    )
    .bind(name)
    .bind(description)
    .bind(settings)
    .bind(&now)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected()
        > 0;
    if let (true, Some(p)) = (found, pictures) {
        sqlx::query(
            "UPDATE badge SET large = ?, small = ?, original = ?, original_mime = ? WHERE id = ?",
        )
        .bind(&p.large)
        .bind(&p.small)
        .bind(&p.original)
        .bind(&p.original_mime)
        .bind(id)
        .execute(pool)
        .await?;
    }
    Ok(found)
}

/// Which of a badge's pictures.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Large,
    Small,
}

impl Size {
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "large" => Some(Self::Large),
            "small" => Some(Self::Small),
            _ => None,
        }
    }
}

/// A finished picture: (bytes, updated_at). An automatic badge nobody has
/// redrawn is drawn from its art.
pub async fn picture(
    pool: &SqlitePool,
    id: i64,
    size: Size,
) -> AppResult<Option<(Vec<u8>, String)>> {
    let sql = match size {
        Size::Large => "SELECT large, updated_at, rule FROM badge WHERE id = ?",
        Size::Small => "SELECT small, updated_at, rule FROM badge WHERE id = ?",
    };
    let row: Option<(Vec<u8>, String, Option<String>)> =
        sqlx::query_as(sql).bind(id).fetch_optional(pool).await?;
    Ok(match row {
        Some((bytes, version, Some(rule))) if bytes.is_empty() => {
            crate::awards::art::picture(rule, size)
                .await
                .map(|b| (b, version))
        }
        Some((bytes, version, _)) => Some((bytes, version)),
        None => None,
    })
}

/// What was uploaded: (bytes, mime). For an automatic badge nobody has
/// redrawn, its large picture, which is where the editor starts from.
pub async fn original(pool: &SqlitePool, id: i64) -> AppResult<Option<(Vec<u8>, String)>> {
    let row: Option<(Vec<u8>, String, Option<String>)> =
        sqlx::query_as("SELECT original, original_mime, rule FROM badge WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(match row {
        Some((bytes, _, Some(rule))) if bytes.is_empty() => {
            crate::awards::art::picture(rule, Size::Large)
                .await
                .map(|b| (b, "image/png".to_string()))
        }
        Some((bytes, mime, _)) => Some((bytes, mime)),
        None => None,
    })
}

/// One holder of a removed badge, as they had it.
#[derive(Serialize, Deserialize)]
pub struct RemovedAward {
    pub user_id: i64,
    pub awarded_at: String,
    pub awarded_by: Option<i64>,
    pub shown: bool,
    pub position: i64,
}

/// Everything a badge was, for undo to put back.
#[derive(Serialize, Deserialize)]
pub struct Removed {
    pub name: String,
    pub description: Option<String>,
    pub settings_json: String,
    pub created_at: String,
    pub pictures: Pictures,
    pub awards: Vec<RemovedAward>,
}

type RemovedRow = (
    String,
    Option<String>,
    String,
    String,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    String,
);

/// Remove a badge; its awards go with it (the key cascades). Answers what
/// it was, or nothing when there was no such badge. One Frog gives on its own
/// stays: it would only come back at the next start, and be earned again.
pub async fn remove(pool: &SqlitePool, id: i64) -> AppResult<Option<Removed>> {
    let rule: Option<(Option<String>,)> = sqlx::query_as("SELECT rule FROM badge WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    if matches!(rule, Some((Some(_),))) {
        return Err(AppError::Conflict(
            "Frog gives this badge on its own. Rename or redraw it instead.".into(),
        ));
    }
    let mut tx = pool.begin().await?;
    // (name, description, settings, created_at, large, small, original, mime)
    let row: Option<RemovedRow> = sqlx::query_as(
        "SELECT name, description, settings_json, created_at, large, small, original,
                original_mime
           FROM badge WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((name, description, settings_json, created_at, large, small, original, mime)) = row
    else {
        return Ok(None);
    };
    let awards: Vec<(i64, String, Option<i64>, bool, i64)> = sqlx::query_as(
        "SELECT a.user_id, a.awarded_at, a.awarded_by, a.shown, a.position
           FROM badge_award a WHERE a.badge_id = ?",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM badge WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Some(Removed {
        name,
        description,
        settings_json,
        created_at,
        pictures: Pictures {
            large,
            small,
            original,
            original_mime: mime,
        },
        awards: awards
            .into_iter()
            .map(
                |(user_id, awarded_at, awarded_by, shown, position)| RemovedAward {
                    user_id,
                    awarded_at,
                    awarded_by,
                    shown,
                    position,
                },
            )
            .collect(),
    }))
}

/// Put a removed badge back, with everyone who still has an account holding
/// it again as they did. Answers its new id.
pub async fn restore(pool: &SqlitePool, removed: &Removed) -> AppResult<i64> {
    let name = check_name(&removed.name)?;
    let description = check_description(removed.description.as_deref())?;
    let settings = check_settings(&removed.settings_json)?;
    let p = &removed.pictures;
    check_picture(&p.large, LARGE_PIXELS, LARGE_MAX_BYTES)?;
    check_picture(&p.small, SMALL_PIXELS, SMALL_MAX_BYTES)?;
    let mime = check_original(&p.original)?;
    if name_taken(pool, &name, None).await? {
        return Err(taken());
    }
    let mut tx = pool.begin().await?;
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO badge (name, description, large, small, original, original_mime,
                            settings_json, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(&name)
    .bind(&description)
    .bind(&p.large)
    .bind(&p.small)
    .bind(&p.original)
    .bind(mime)
    .bind(&settings)
    .bind(&removed.created_at)
    .bind(Utc::now().stamp())
    .fetch_one(&mut *tx)
    .await?;
    for a in &removed.awards {
        sqlx::query(
            "INSERT INTO badge_award (user_id, badge_id, awarded_at, awarded_by, shown, position)
             SELECT u.id, ?, ?, (SELECT b.id FROM app_user b WHERE b.id = ?), ?, ?
               FROM app_user u WHERE u.id = ?",
        )
        .bind(id)
        .bind(&a.awarded_at)
        .bind(a.awarded_by)
        .bind(a.shown)
        .bind(a.position)
        .bind(a.user_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(id)
}

// --- holding one --------------------------------------------------------

/// Give a badge to someone, after the ones they hold. Says whether it is new
/// to them: giving it twice changes nothing.
pub async fn award(pool: &SqlitePool, badge_id: i64, user_id: i64, by: i64) -> AppResult<bool> {
    let added = sqlx::query(
        "INSERT INTO badge_award (user_id, badge_id, awarded_at, awarded_by, shown, position)
         VALUES (?, ?, ?, ?, 1,
                 COALESCE((SELECT MAX(position) FROM badge_award WHERE user_id = ?), -1) + 1)
         ON CONFLICT (user_id, badge_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(badge_id)
    .bind(Utc::now().stamp())
    .bind(by)
    .bind(user_id)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(added > 0)
}

/// Take a badge back. Says whether they had it.
pub async fn revoke(pool: &SqlitePool, badge_id: i64, user_id: i64) -> AppResult<bool> {
    Ok(
        sqlx::query("DELETE FROM badge_award WHERE user_id = ? AND badge_id = ?")
            .bind(user_id)
            .bind(badge_id)
            .execute(pool)
            .await?
            .rows_affected()
            > 0,
    )
}

/// A badge as its holder has it.
#[derive(Serialize, Clone)]
pub struct Held {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub awarded_at: String,
    pub shown: bool,
    /// When its pictures last changed, for their addresses.
    pub version: String,
    /// For one Frog gave on its own: its catalogue key, the day it was
    /// earned, and the title it was earned with, when there is one.
    pub rule: Option<String>,
    pub earned_on: Option<String>,
    pub earned_with: Option<String>,
    /// For one made here, what earns it on its own, as kept.
    pub condition: Option<String>,
}

/// (id, name, description, awarded_at, shown, version, rule, earned_on,
///  condition, earned with)
type HeldRow = (
    i64,
    String,
    Option<String>,
    String,
    bool,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// How many accounts that can sign in hold each badge, and how many such
/// accounts there are: the badges page's "rare" sort and its "held by" line.
/// Counts only, never whose.
pub async fn holders(pool: &SqlitePool) -> AppResult<(std::collections::HashMap<i64, i64>, i64)> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT a.badge_id, COUNT(*) FROM badge_award a
           JOIN app_user u ON u.id = a.user_id
          WHERE u.disabled = 0
          GROUP BY a.badge_id",
    )
    .fetch_all(pool)
    .await?;
    let (accounts,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM app_user WHERE disabled = 0")
        .fetch_one(pool)
        .await?;
    Ok((rows.into_iter().collect(), accounts))
}

/// The badges one account holds, in its order: every one for the holder
/// themselves, only the shown ones for anybody else.
pub async fn held(pool: &SqlitePool, user_id: i64, with_hidden: bool) -> AppResult<Vec<Held>> {
    let rows: Vec<HeldRow> = sqlx::query_as(
        "SELECT b.id, b.name, b.description, a.awarded_at, a.shown, b.updated_at, b.rule,
                a.earned_on, b.condition,
                (SELECT t.name FROM title t
                  WHERE t.id = json_extract(a.evidence, '$.title'))
           FROM badge_award a JOIN badge b ON b.id = a.badge_id
          WHERE a.user_id = ? AND (a.shown = 1 OR ?)
          ORDER BY a.position, a.awarded_at",
    )
    .bind(user_id)
    .bind(with_hidden)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                id,
                name,
                description,
                awarded_at,
                shown,
                version,
                rule,
                earned_on,
                condition,
                earned_with,
            )| {
                Held {
                    condition,
                    id,
                    name,
                    description,
                    awarded_at,
                    shown,
                    version,
                    rule,
                    earned_on,
                    earned_with,
                }
            },
        )
        .collect())
}

/// The large picture of a badge someone holds, at its address, for a
/// notification: what the page and the push show instead of a crown.
pub async fn held_picture(
    pool: &SqlitePool,
    user_id: i64,
    username: &str,
    badge_id: i64,
) -> AppResult<Option<String>> {
    Ok(held(pool, user_id, true)
        .await?
        .iter()
        .find(|h| h.id == badge_id)
        .map(|h| address(username, h, Size::Large)))
}

/// Where a held badge's picture is served: through the holder's profile, so
/// only someone who may see that profile can load it. The version changes
/// with the picture, which lets the answer be cached for good.
pub fn address(username: &str, held: &Held, size: Size) -> String {
    format!(
        "/api/profiles/{}/badges/{}/{}?v={}",
        urlencoding::encode(username),
        held.id,
        match size {
            Size::Large => "large",
            Size::Small => "small",
        },
        urlencoding::encode(&held.version)
    )
}

/// A badge Frog gives on its own, as the list of everything there is to earn
/// shows it.
pub struct Automatic {
    pub id: i64,
    pub rule: String,
    pub name: String,
    pub description: Option<String>,
    pub version: String,
}

/// Every badge Frog gives on its own that has its row, so not one waiting
/// for a name.
pub async fn automatic(pool: &SqlitePool) -> AppResult<Vec<Automatic>> {
    let rows: Vec<(i64, String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT id, rule, name, description, updated_at FROM badge WHERE rule IS NOT NULL",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, rule, name, description, version)| Automatic {
            id,
            rule,
            name,
            description,
            version,
        })
        .collect())
}

/// A badge made here that Frog gives on its own once its condition is met.
pub struct Conditional {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    /// When its pictures last changed, for their addresses.
    pub version: String,
    pub condition: Condition,
}

/// Every badge made here with a condition Frog can count.
pub async fn conditional(pool: &SqlitePool) -> AppResult<Vec<Conditional>> {
    let rows: Vec<(i64, String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT id, name, description, updated_at, condition FROM badge
          WHERE condition IS NOT NULL AND rule IS NULL
          ORDER BY name COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, name, description, version, condition)| {
            Some(Conditional {
                id,
                name,
                description,
                version,
                condition: Condition::parse(&condition)?,
            })
        })
        .collect())
}

/// What earns a badge made here on its own, or nothing for one only ever
/// given by hand. Frog's own badges keep their rules; false for those and
/// for a badge that is not there.
pub async fn set_condition(
    pool: &SqlitePool,
    id: i64,
    condition: Option<&Condition>,
) -> AppResult<bool> {
    let text = condition.map(|c| serde_json::json!(c).to_string());
    let done =
        sqlx::query("UPDATE badge SET condition = ?, updated_at = ? WHERE id = ? AND rule IS NULL")
            .bind(text)
            .bind(Utc::now().stamp())
            .bind(id)
            .execute(pool)
            .await?;
    Ok(done.rows_affected() > 0)
}

/// Whether a badge is one anyone may earn, Frog's own or one made here with a
/// condition: anyone signed in may see those, earned or not.
pub async fn is_automatic(pool: &SqlitePool, id: i64) -> AppResult<bool> {
    let (yes,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM badge
                         WHERE id = ? AND (rule IS NOT NULL OR condition IS NOT NULL))",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(yes)
}

/// The holder's name, for the address a notification opens.
pub async fn holder_name(pool: &SqlitePool, user_id: i64) -> AppResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT username FROM app_user WHERE id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(n,)| n))
}

/// A catalogue badge by its key, and whether this account holds it:
/// (id, name, description, held).
pub async fn by_rule(
    pool: &SqlitePool,
    user_id: i64,
    rule: &str,
) -> AppResult<Option<(i64, String, Option<String>, bool)>> {
    Ok(sqlx::query_as(
        "SELECT b.id, b.name, b.description,
                EXISTS (SELECT 1 FROM badge_award a WHERE a.badge_id = b.id AND a.user_id = ?)
           FROM badge b WHERE b.rule = ?",
    )
    .bind(user_id)
    .bind(rule)
    .fetch_optional(pool)
    .await?)
}

/// Whether one account holds a badge. Anyone who may see the profile may
/// see every badge on it: `shown` only says which sit beside the name.
pub async fn seen(pool: &SqlitePool, user_id: i64, badge_id: i64) -> AppResult<bool> {
    let (seen,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM badge_award WHERE user_id = ? AND badge_id = ?)",
    )
    .bind(user_id)
    .bind(badge_id)
    .fetch_one(pool)
    .await?;
    Ok(seen)
}

/// The holder's order and which show, as Settings sends it: every badge
/// named, in order. Badges left out keep their place after the ones named.
pub async fn arrange(pool: &SqlitePool, user_id: i64, order: &[(i64, bool)]) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    for (position, (badge_id, shown)) in order.iter().enumerate() {
        sqlx::query(
            "UPDATE badge_award SET position = ?, shown = ? WHERE user_id = ? AND badge_id = ?",
        )
        .bind(position as i64)
        .bind(shown)
        .bind(user_id)
        .bind(badge_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// (name, awarded_at) of every badge one account holds, for its export.
pub async fn for_export(pool: &SqlitePool, user_id: i64) -> AppResult<Vec<(String, String)>> {
    Ok(sqlx::query_as(
        "SELECT b.name, a.awarded_at FROM badge_award a JOIN badge b ON b.id = a.badge_id
          WHERE a.user_id = ? ORDER BY a.position, a.awarded_at",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(width, height, image::Rgba([10, 200, 90, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn only_square_pngs_of_the_exact_size_pass() {
        assert!(check_picture(&png(128, 128), SMALL_PIXELS, SMALL_MAX_BYTES).is_ok());
        let wide = check_picture(&png(128, 64), SMALL_PIXELS, SMALL_MAX_BYTES).unwrap_err();
        assert!(wide.to_string().contains("square"), "{wide}");
        let off = check_picture(&png(120, 120), SMALL_PIXELS, SMALL_MAX_BYTES).unwrap_err();
        assert!(off.to_string().contains("128 pixels"), "{off}");
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0, 0, 0, 0];
        assert!(check_picture(&jpeg, SMALL_PIXELS, SMALL_MAX_BYTES)
            .unwrap_err()
            .to_string()
            .contains("PNG"));
        // A header that lies about what follows.
        let mut broken = png(128, 128);
        broken.truncate(40);
        assert!(check_picture(&broken, SMALL_PIXELS, SMALL_MAX_BYTES).is_err());
        let big = png(128, 128);
        assert!(check_picture(&big, SMALL_PIXELS, big.len() - 1).is_err());
    }

    #[test]
    fn an_svg_is_never_kept() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><script/></svg>"#;
        assert!(check_original(svg).is_err());
        assert_eq!(check_original(&png(4, 4)).unwrap(), "image/png");
    }

    #[test]
    fn names_and_descriptions_are_one_short_line() {
        assert_eq!(check_name("  FEM BOY ").unwrap(), "FEM BOY");
        assert!(check_name("   ").is_err());
        assert!(check_name(&"x".repeat(NAME_MAX + 1)).is_err());
        assert!(check_name("two\nlines").is_err());
        assert_eq!(check_description(Some("  ")).unwrap(), None);
        assert!(check_description(Some("a\nb")).is_err());
        assert!(check_settings("[1]").is_err());
        assert_eq!(
            check_settings(r#"{"zoom":1.5}"#).unwrap(),
            r#"{"zoom":1.5}"#
        );
    }
}
