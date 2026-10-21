#[cfg(target_arch = "wasm32")]
mod gpu_compute_worker;
mod gpu_proxy;
mod gpu_wire;
#[cfg(target_arch = "wasm32")]
mod wgpu_presenter;
mod adaptive_clock;
mod app;
mod bench;
mod browser_bridge;
mod catalogue;
#[cfg(test)]
mod compact_vectors;
mod components;
mod emulator;
mod host_cursor;
mod indexed_frame;
mod paths;
mod presentation;
mod renderer_bridge;
mod save_store;
mod worker_runtime;

use leptos::mount::{mount_to, mount_to_body};
use wasm_bindgen::JsCast;

fn main() {
    if web_sys::window().is_none() {
        return;
    }
    if catalogue::games().is_empty() {
        return;
    }
    // Calibrate saved page zoom while navigating the catalogue or booting a game.
    browser_bridge::systemless_cursor_zoom();
    if let Some(root) = root_element() {
        root.set_inner_html("");
        mount_to(root, app::App).forget();
    } else {
        mount_to_body(app::App);
    }
}

fn root_element() -> Option<web_sys::HtmlElement> {
    web_sys::window()?
        .document()?
        .get_element_by_id("root")?
        .dyn_into()
        .ok()
}
