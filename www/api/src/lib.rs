use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use worker::*;

const DAY_MS: u64 = 86_400_000;
const MAX_BODY: usize = 512;
const CACHE_CONTROL: &str = "public, max-age=300, s-maxage=3600";

#[derive(Default, Serialize)]
struct Popularity {
    plays: BTreeMap<String, u64>,
}

#[derive(Deserialize)]
struct Play {
    game: String,
    visitor: String,
}

impl Play {
    fn valid(&self) -> bool {
        let game = self.game.as_bytes();
        !game.is_empty()
            && game.len() <= 100
            && game[0].is_ascii_alphanumeric()
            && game
                .iter()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
            && self.visitor.len() == 32
            && self
                .visitor
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }
}

fn utc_day(millis: u64) -> String {
    DateTime::<Utc>::from_timestamp_millis(millis as i64)
        .expect("current timestamp must be representable")
        .format("%Y-%m-%d")
        .to_string()
}

fn reply(status: u16) -> Result<Response> {
    let mut response = Response::empty()?.with_status(status);
    response.headers_mut().set("Cache-Control", "no-store")?;
    Ok(response)
}

fn json<T: Serialize>(body: &T, status: u16, cache: &str) -> Result<Response> {
    let mut response = Response::from_json(body)?.with_status(status);
    response.headers_mut().set("Cache-Control", cache)?;
    Ok(response)
}

#[event(fetch)]
pub async fn fetch(mut request: Request, env: Env, ctx: Context) -> Result<Response> {
    let url = request.url()?;
    match url.path() {
        "/api/popularity" => {
            if request.method() != Method::Get {
                return json(
                    &serde_json::json!({"error": "Method not allowed"}),
                    405,
                    "no-store",
                );
            }
            if url.host_str() != Some("systemless.org") || env.d1("POPULARITY").is_err() {
                return json(&Popularity::default(), 200, "no-store");
            }
            match rankings(&env, ctx, &url.origin().ascii_serialization()).await {
                Ok(response) => Ok(response),
                Err(_) => json(&Popularity::default(), 503, "no-store"),
            }
        }
        "/api/play" => {
            if request.method() != Method::Post {
                return reply(405);
            }
            if url.host_str() != Some("systemless.org") || env.d1("POPULARITY").is_err() {
                return reply(503);
            }
            if request.headers().get("Origin")?.as_deref()
                != Some(url.origin().ascii_serialization().as_str())
            {
                return reply(403);
            }
            if request.headers().get("Content-Type")?.as_deref() != Some("application/json") {
                return reply(415);
            }
            match record_play(&mut request, &env, &url.origin().ascii_serialization()).await {
                Ok(response) => Ok(response),
                Err(_) => reply(503),
            }
        }
        // `_routes.json` normally bypasses the Worker for static requests. Keep
        // the fallback so Pages assets also work when invoked directly in tests.
        _ => env.service("ASSETS")?.fetch_request(request).await,
    }
}

async fn rankings(env: &Env, ctx: Context, origin: &str) -> Result<Response> {
    // A single cache key per origin, regardless of query strings.
    let key = format!("{origin}/api/popularity");
    let cache = Cache::default();
    if let Some(response) = cache.get(key.as_str(), false).await? {
        return Ok(response);
    }
    #[derive(Deserialize)]
    struct Row {
        game: String,
        plays: u64,
    }
    let since = utc_day(Date::now().as_millis() - 29 * DAY_MS);
    let rows = env
        .d1("POPULARITY")?
        .prepare("SELECT game, SUM(plays) AS plays FROM daily_plays WHERE day >= ? GROUP BY game")
        .bind(&[since.into()])?
        .all()
        .await?
        .results::<Row>()?;
    let counts = Popularity {
        plays: rows.into_iter().map(|row| (row.game, row.plays)).collect(),
    };
    let mut response = json(&counts, 200, CACHE_CONTROL)?;
    let cached = response.cloned()?;
    ctx.wait_until(async move {
        let _ = cache.put(key.as_str(), cached).await;
    });
    Ok(response)
}

async fn record_play(request: &mut Request, env: &Env, origin: &str) -> Result<Response> {
    // Check streamed bytes, not just Content-Length, before parsing JSON. Dropping
    // the stream cancels the reader on oversized or disconnected requests.
    let mut stream = match request.stream() {
        Ok(stream) => stream,
        Err(_) => return reply(400),
    };
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(_) => return reply(400),
        };
        if body.len().saturating_add(chunk.len()) > MAX_BODY {
            return reply(413);
        }
        body.extend_from_slice(&chunk);
    }
    let play = match serde_json::from_slice::<Play>(&body) {
        Ok(play) if play.valid() => play,
        _ => return reply(400),
    };
    let mut catalogue = env
        .service("ASSETS")?
        .fetch(format!("{origin}/popularity-games.json"), None)
        .await?;
    if !(200..300).contains(&catalogue.status_code()) {
        return reply(503);
    }
    if !catalogue.json::<Vec<String>>().await?.contains(&play.game) {
        return reply(400);
    }
    let now = Date::now().as_millis();
    let day = utc_day(now);
    let oldest = utc_day(now - 29 * DAY_MS);
    let db = env.d1("POPULARITY")?;
    // Preserve the existing atomic deduplication trigger and daily per-ID cap.
    db.batch(vec![
        db.prepare(
            "DELETE FROM play_visitors WHERE (day, visitor, game) IN \
            (SELECT day, visitor, game FROM play_visitors WHERE day < ? LIMIT 100)",
        )
        .bind(&[day.clone().into()])?,
        db.prepare(
            "DELETE FROM daily_plays WHERE (day, game) IN \
            (SELECT day, game FROM daily_plays WHERE day < ? LIMIT 100)",
        )
        .bind(&[oldest.into()])?,
        db.prepare(
            "INSERT OR IGNORE INTO play_visitors (day, visitor, game) \
            SELECT ?, ?, ? WHERE \
            (SELECT COUNT(*) FROM play_visitors WHERE day = ? AND visitor = ?) < 50",
        )
        .bind(&[
            day.clone().into(),
            play.visitor.clone().into(),
            play.game.into(),
            day.into(),
            play.visitor.into(),
        ])?,
    ])
    .await?;
    reply(204)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_window_crosses_month_and_year_boundaries() {
        let now = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .timestamp_millis() as u64;
        assert_eq!(utc_day(now), "2026-01-01");
        assert_eq!(utc_day(now - 29 * DAY_MS), "2025-12-03");
        assert_eq!(utc_day(now - 1), "2025-12-31");
    }

    #[test]
    fn validates_game_and_visitor_without_coercion() {
        let visitor = "a".repeat(32);
        for game in ["doom", "3-d-pinball", &"a".repeat(100)] {
            assert!(Play {
                game: game.into(),
                visitor: visitor.clone()
            }
            .valid());
        }
        for game in [
            "",
            "-doom",
            "Doom",
            "../doom",
            "doom\n",
            "dööm",
            &"a".repeat(101),
        ] {
            assert!(!Play {
                game: game.into(),
                visitor: visitor.clone()
            }
            .valid());
        }
        for bad in ["a".repeat(31), "A".repeat(32), "g".repeat(32)] {
            assert!(!Play {
                game: "doom".into(),
                visitor: bad
            }
            .valid());
        }
        assert!(serde_json::from_str::<Play>(r#"{"game":123,"visitor":"a"}"#).is_err());
    }
}
