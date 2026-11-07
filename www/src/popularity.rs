use std::collections::BTreeMap;

use crate::catalogue::Game;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/src/popularity.js")]
extern "C" {
    #[wasm_bindgen(js_name = startPlay)]
    pub fn start_play(canvas: &web_sys::HtmlCanvasElement, game: &str);
    #[wasm_bindgen(js_name = playHeartbeat)]
    pub fn play_heartbeat(canvas: &web_sys::HtmlCanvasElement, running: bool);
}

#[derive(Default, serde::Deserialize)]
pub struct Popularity {
    pub plays: BTreeMap<String, u64>,
}

pub async fn fetch() -> Popularity {
    async fn load() -> Option<Popularity> {
        let response = gloo_net::http::Request::get("/api/popularity")
            .send()
            .await
            .ok()?;
        if !response.ok() {
            return None;
        }
        response.json().await.ok()
    }
    load().await.unwrap_or_default()
}

pub fn sort(games: &mut [&Game], popularity: &Popularity, alphabetical: bool) {
    games.sort_by_cached_key(|game| {
        (
            std::cmp::Reverse(if alphabetical {
                0
            } else {
                *popularity.plays.get(game.id).unwrap_or(&0)
            }),
            game.title.to_lowercase(),
            game.id,
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn popularity_defaults_to_zero_and_breaks_ties_by_title() {
        let mut games: Vec<_> = crate::catalogue::games().iter().collect();
        sort(&mut games, &Popularity::default(), false);
        assert!(games
            .windows(2)
            .all(|pair| pair[0].title.to_lowercase() <= pair[1].title.to_lowercase()));
        let last = games.last().unwrap().id;
        let popularity = Popularity {
            plays: [(last.to_owned(), 1)].into(),
        };
        sort(&mut games, &popularity, false);
        assert_eq!(games[0].id, last);
        sort(&mut games, &popularity, true);
        assert_eq!(games.last().unwrap().id, last);
    }
}
