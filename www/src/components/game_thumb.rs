use leptos::prelude::*;

use crate::catalogue::Game;
use crate::paths::asset_path;

#[component]
pub fn GameThumb(game: &'static Game) -> impl IntoView {
    let src = asset_path(game.assets.screenshot_path);
    view! {
        <img
            class="game-thumb"
            src=src
            alt=format!("{} screenshot", game.title)
            draggable="false"
        />
    }
}
