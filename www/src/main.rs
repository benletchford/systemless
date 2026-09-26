mod app;
mod bench;
mod browser_bridge;
mod catalogue;
#[cfg(test)]
mod compact_vectors;
mod components;
mod emulator;
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
