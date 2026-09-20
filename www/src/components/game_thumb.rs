use leptos::prelude::*;

use crate::catalogue::Game;
use crate::paths::optional_asset_path;

#[component]
pub fn GameThumb(game: &'static Game) -> impl IntoView {
    if let Some(src) = optional_asset_path(game.assets.screenshot_path) {
        view! {
            <img
                class="game-thumb"
                src=src
                alt=format!("{} screenshot", game.title)
                draggable="false"
            />
        }
        .into_any()
    } else {
        view! {
            <span class="game-thumb game-thumb--placeholder" aria-hidden="true">
                <span class="game-thumb__placeholder-mark">"S"</span>
            </span>
        }
        .into_any()
    }
}
