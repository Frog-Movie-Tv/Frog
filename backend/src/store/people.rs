//! Accounts as other accounts see them: who may be shown, and how a person
//! is named and pictured anywhere one account sees another.

use super::*;

/// An account as someone else sees it, for anything that names a person.
#[derive(Debug, Clone, Serialize)]
pub struct Shown {
    pub username: String,
    pub display_name: Option<String>,
    pub avatar: Option<String>,
}

/// An account another may see, with the id that reaches its data.
#[derive(Debug, Clone)]
pub struct ShownAccount {
    pub id: i64,
    pub shown: Shown,
}

/// A profile picture's address. The time in it makes a new picture show at once.
pub fn picture_address(username: &str, kind: &str, updated_at: &str) -> String {
    format!(
        "/api/profiles/{}/images/{kind}?v={}",
        urlencoding::encode(username),
        urlencoding::encode(updated_at)
    )
}

/// Everyone a viewer may see: switched on, profile on, and not themselves. The
/// same test `routes::profiles` makes of one account at a time.
pub async fn shown_to(pool: &SqlitePool, viewer: i64) -> AppResult<Vec<ShownAccount>> {
    let rows: Vec<(i64, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT u.id, u.username, u.display_name,
                (SELECT i.updated_at FROM user_image i
                  WHERE i.user_id = u.id AND i.kind = 'avatar')
           FROM app_user u
          WHERE u.disabled = 0 AND u.profile_public = 1 AND u.id <> ?",
    )
    .bind(viewer)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, username, display_name, picture)| ShownAccount {
            id,
            shown: Shown {
                avatar: picture.map(|at| picture_address(&username, "avatar", &at)),
                username,
                display_name,
            },
        })
        .collect())
}

/// The viewer, named as Activity names everyone else beside them.
pub async fn myself(pool: &SqlitePool, viewer: i64) -> AppResult<ShownAccount> {
    let (username, display_name, picture): (String, Option<String>, Option<String>) =
        sqlx::query_as(
            "SELECT u.username, u.display_name,
                    (SELECT i.updated_at FROM user_image i
                      WHERE i.user_id = u.id AND i.kind = 'avatar')
               FROM app_user u
              WHERE u.id = ?",
        )
        .bind(viewer)
        .fetch_one(pool)
        .await?;
    Ok(ShownAccount {
        id: viewer,
        shown: Shown {
            avatar: picture.map(|at| picture_address(&username, "avatar", &at)),
            username,
            display_name,
        },
    })
}
