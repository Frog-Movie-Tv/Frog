//! The Airing page: a week of every show on air and film out, a day at a
//! time, most popular first. Read from what the worker keeps (`crate::airing`); nothing here
//! reaches another site.

use axum::extract::{Query, State};
use axum::Json;
use chrono::{Duration, NaiveDate, Utc};
use chrono_tz::Tz;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::airing::{self, Filters};
use crate::auth::Auth;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Shows a day shows before "Show more".
const PAGE: i64 = 30;
const MOST: i64 = 60;

#[derive(Debug, Deserialize)]
pub struct WeekQuery {
    /// First day, yyyy-mm-dd where the viewer is. Defaults to today.
    pub from: Option<String>,
    /// The viewer's IANA zone. The account's own, when it has one, else this.
    pub tz: Option<String>,
    /// `mine` for the viewer's library only.
    pub list: Option<String>,
    /// `shows`, `anime` or `films`.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub genre: Option<String>,
    pub q: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DayQuery {
    pub date: String,
    pub tz: Option<String>,
    pub list: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub genre: Option<String>,
    pub q: Option<String>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

/// The zone days are counted in: the one the page sends, which is the
/// account's chosen zone or the browser's, else the one notifications use.
async fn zone(state: &AppState, user_id: i64, sent: Option<&str>) -> Tz {
    match sent.and_then(|z| z.parse::<Tz>().ok()) {
        Some(zone) => zone,
        None => crate::announce::time_zone(&state.pool, user_id).await,
    }
}

fn filters(
    list: Option<&str>,
    kind: Option<&str>,
    genre: Option<&str>,
    q: Option<&str>,
) -> Filters {
    let clean = |s: Option<&str>| {
        s.map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.chars().take(80).collect::<String>())
    };
    Filters {
        mine: list == Some("mine"),
        kind: kind.and_then(airing::Kind::parse),
        genre: clean(genre),
        search: clean(q).map(|q| crate::text::fold(&q)),
    }
}

fn parse_day(raw: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("A day is written yyyy-mm-dd.".into()))
}

/// Seven days from `from`, each with its first shows and how many there are.
pub async fn week(
    auth: Auth,
    State(state): State<AppState>,
    Query(q): Query<WeekQuery>,
) -> AppResult<Json<Value>> {
    let zone = zone(&state, auth.user_id(), q.tz.as_deref()).await;
    let today = Utc::now().with_timezone(&zone).date_naive();
    let (first, last) = airing::window(today);
    let from = match q.from.as_deref() {
        Some(raw) => parse_day(raw)?,
        None => today,
    }
    .clamp(first, last);
    let filters = filters(
        q.list.as_deref(),
        q.kind.as_deref(),
        q.genre.as_deref(),
        q.q.as_deref(),
    );

    let mut days = Vec::with_capacity(7);
    let mut day = from;
    while day <= last && days.len() < 7 {
        days.push(airing::day(&state.pool, auth.user_id(), zone, day, &filters, 0, PAGE).await?);
        day += Duration::days(1);
    }
    let (listed_at, shows) = airing::status(&state.pool).await?;
    Ok(Json(json!({
        "zone": zone.name(),
        "today": today.to_string(),
        "first": first.to_string(),
        "last": last.to_string(),
        "from": from.to_string(),
        "ready": shows > 0,
        "listed_at": listed_at,
        "genres": airing::genres(&state.pool).await?,
        "days": days,
    })))
}

/// More of one day, for "Show more".
pub async fn day(
    auth: Auth,
    State(state): State<AppState>,
    Query(q): Query<DayQuery>,
) -> AppResult<Json<airing::Day>> {
    let zone = zone(&state, auth.user_id(), q.tz.as_deref()).await;
    let date = parse_day(&q.date)?;
    let filters = filters(
        q.list.as_deref(),
        q.kind.as_deref(),
        q.genre.as_deref(),
        q.q.as_deref(),
    );
    let offset = q.offset.unwrap_or(0).max(0);
    let limit = q.limit.unwrap_or(PAGE).clamp(1, MOST);
    Ok(Json(
        airing::day(
            &state.pool,
            auth.user_id(),
            zone,
            date,
            &filters,
            offset,
            limit,
        )
        .await?,
    ))
}
