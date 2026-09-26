use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use js_sys::{Array, Object, Reflect, Uint8Array};
use leptos::prelude::*;
use leptos::task::{spawn_local, spawn_local_scoped_with_cancellation};
use systemless::debug_overlay::DebugOverlayFrameStats;
use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AddEventListenerOptions, CssStyleDeclaration, Element, Event, HtmlCanvasElement, HtmlElement,
    HtmlInputElement, KeyboardEvent, MessageEvent, MouseEvent, PointerEvent,
    ReadableStreamDefaultReader, ReadableStreamReadResult, Touch, TouchEvent, TouchList, Worker,
};

use crate::browser_bridge::{
    acknowledge_systemless_worker_command, boot_systemless_worker, cancel_systemless_worker,
    fetch_archive_in_worker, post_systemless_worker_command, prefetch_archive, prefetched_archive,
    shutdown_systemless_worker, systemless_runtime_assets, wait_systemless_worker_shutdown,
};
use crate::catalogue::{Game, GameArchitecture, GamePlugin, MobileControlButton, MobileControls};
use crate::emulator::{
    AudioBootstrap, BootProgress, FrameRunResult, Machine, PerfCounters, PluginFile,
    WebAudioBackend,
};
use crate::paths::asset_path;
use crate::presentation::CanvasFrame;
use crate::save_store::{self, DownloadableSaveFile};

// Some games poll Button/MBState instead of consuming mouseDown events. A quick
// tap can otherwise press and release between emulator frames.
const MIN_TOUCH_PRESS_MS: f64 = 90.0;
const FETCH_COPY_YIELD_BYTES: u32 = 256 * 1024;
const FETCH_COPY_FRAME_YIELD_BYTES: u32 = 1024 * 1024;
const PLUGIN_SELECTION_STORAGE_PREFIX: &str = "systemless.plugin-selection.";

pub fn prefetch_game_archive(game: &Game) {
    if !game.approved || game.assets.archive_path.is_empty() {
        return;
    }
    prefetch_archive(&primary_archive_url(game));
}

#[component]
pub fn GameScreen(game: &'static Game) -> impl IntoView {
    let selected_plugins = RwSignal::new(load_selected_plugin_ids(game));
    let selected_plugins_for_storage = selected_plugins;
    let selected_architecture = RwSignal::new(game.default_architecture);

    Effect::new(move |_| {
        persist_selected_plugin_ids(game, &selected_plugins_for_storage.get());
    });

    view! {
        {if game.architectures.len() > 1 {
            view! {
                <div class="game-architecture" role="group" aria-label="Choose game architecture">
                    <span class="game-architecture__label">"Run as"</span>
                    <div class="cli-platform">
                        {game.architectures.iter().copied().map(|architecture| view! {
                            <button
                                type="button"
                                class=move || architecture_selector_class(
                                    selected_architecture.get() == architecture,
                                )
                                aria-pressed=move || bool_attr(
                                    selected_architecture.get() == architecture,
                                )
                                on:click=move |_| {
                                    if selected_architecture.get_untracked() != architecture {
                                        crate::emulator::begin_audio_from_user_gesture();
                                        selected_architecture.set(architecture);
                                    }
                                }
                            >
                                {architecture.label()}
                            </button>
                        }).collect_view()}
                    </div>
                </div>
            }.into_any()
        } else {
            view! {}.into_any()
        }}
        <For
            each=move || vec![selected_plugins.get()]
            key=plugin_instance_key
            let:plugin_ids
        >
            <For
                each=move || vec![selected_architecture.get()]
                key=|architecture| architecture.key()
                let:architecture
            >
                <GameRuntime
                    game=game
                    architecture=architecture
                    selected_plugin_ids=plugin_ids.clone()
                    selected_plugins_signal=selected_plugins
                />
            </For>
        </For>
    }
}

fn architecture_selector_class(active: bool) -> &'static str {
    if active {
        "cli-platform__option cli-platform__option--active"
    } else {
        "cli-platform__option"
    }
}

fn plugin_instance_key(plugin_ids: &Vec<&'static str>) -> String {
    if plugin_ids.is_empty() {
        "none".to_string()
    } else {
        plugin_ids.join("+")
    }
}

fn plugin_selection_storage_key(game_id: &str) -> String {
    format!("{PLUGIN_SELECTION_STORAGE_PREFIX}{game_id}")
}

fn load_selected_plugin_ids(game: &'static Game) -> Vec<&'static str> {
    let Some(storage) = web_sys::window()
        .and_then(|window| window.local_storage().ok())
        .flatten()
    else {
        return Vec::new();
    };
    let Ok(Some(json)) = storage.get_item(&plugin_selection_storage_key(game.id)) else {
        return Vec::new();
    };
    let Ok(saved_ids) = serde_json::from_str::<Vec<String>>(&json) else {
        return Vec::new();
    };
    normalize_plugin_selection(
        game,
        &saved_ids.iter().map(String::as_str).collect::<Vec<&str>>(),
    )
}

fn persist_selected_plugin_ids(game: &'static Game, plugin_ids: &[&'static str]) {
    let Some(storage) = web_sys::window()
        .and_then(|window| window.local_storage().ok())
        .flatten()
    else {
        return;
    };
    let key = plugin_selection_storage_key(game.id);
    if plugin_ids.is_empty() {
        let _ = storage.remove_item(&key);
        return;
    }
    if let Ok(json) = serde_json::to_string(plugin_ids) {
        let _ = storage.set_item(&key, &json);
    }
}

fn normalize_plugin_selection(game: &'static Game, requested_ids: &[&str]) -> Vec<&'static str> {
    game.settings
        .plugins
        .iter()
        .filter(|plugin| {
            !plugin.install_assets.is_empty()
                && requested_ids
                    .iter()
                    .any(|requested_id| *requested_id == plugin.id)
        })
        .map(|plugin| plugin.id)
        .collect()
}

fn set_plugin_selected(
    game: &'static Game,
    selected_plugins_signal: RwSignal<Vec<&'static str>>,
    plugin_id: &'static str,
    checked: bool,
) {
    selected_plugins_signal.update(|ids| {
        let mut requested = ids.iter().copied().collect::<Vec<&str>>();
        if checked {
            requested.push(plugin_id);
        } else {
            requested.retain(|id| *id != plugin_id);
        }
        let next = normalize_plugin_selection(game, &requested);
        if game.approved && *ids != next {
            crate::emulator::begin_audio_from_user_gesture();
        }
        *ids = next;
    });
}

#[derive(Clone)]
enum RuntimeHandle {
    Local(Rc<RefCell<Machine>>),
    Worker(Rc<WorkerRuntime>),
}

impl RuntimeHandle {
    fn key_down(&self, mac_key: u8, char_code: u8) {
        self.mobile_key(true, mac_key, char_code);
    }

    fn key_up(&self, mac_key: u8, char_code: u8) {
        self.mobile_key(false, mac_key, char_code);
    }

    fn mouse_down(&self, v: i16, h: i16) {
        self.mouse("mouseDown", v, h);
    }

    fn mouse_up(&self, v: i16, h: i16) {
        self.mouse("mouseUp", v, h);
    }

    fn mouse_move(&self, v: i16, h: i16) {
        self.mouse("mouseMove", v, h);
    }

    fn mouse(&self, kind: &str, v: i16, h: i16) {
        match self {
            Self::Local(machine) => {
                let mut machine = machine.borrow_mut();
                match kind {
                    "mouseDown" => machine.mouse_down(v, h),
                    "mouseUp" => machine.mouse_up(v, h),
                    _ => machine.mouse_move(v, h),
                }
            }
            Self::Worker(runtime) => {
                let message = Object::new();
                set_js_property(&message, "type", &JsValue::from_str(kind));
                set_js_property(&message, "v", &JsValue::from_f64(v as f64));
                set_js_property(&message, "h", &JsValue::from_f64(h as f64));
                let _ = runtime.post(&message);
            }
        }
    }

    fn resume_audio(&self) {
        match self {
            Self::Local(machine) => machine.borrow_mut().resume_audio(),
            Self::Worker(runtime) => {
                runtime
                    .audio
                    .borrow_mut()
                    .as_mut()
                    .map(WebAudioBackend::resume);
            }
        }
    }

    fn mobile_key(&self, down: bool, mac_key: u8, char_code: u8) {
        match self {
            Self::Local(machine) => {
                let mut machine = machine.borrow_mut();
                if down {
                    machine.key_down(mac_key, char_code);
                } else {
                    machine.key_up(mac_key, char_code);
                }
            }
            Self::Worker(runtime) => {
                let message = Object::new();
                set_js_property(
                    &message,
                    "type",
                    &JsValue::from_str(if down { "keyDown" } else { "keyUp" }),
                );
                set_js_property(&message, "macKey", &JsValue::from_f64(mac_key as f64));
                set_js_property(&message, "charCode", &JsValue::from_f64(char_code as f64));
                let _ = runtime.post(&message);
            }
        }
    }
}

#[component]
fn GameRuntime(
    game: &'static Game,
    architecture: GameArchitecture,
    selected_plugin_ids: Vec<&'static str>,
    selected_plugins_signal: RwSignal<Vec<&'static str>>,
) -> impl IntoView {
    let canvas_ref: NodeRef<leptos::html::Canvas> = NodeRef::new();
    let controls_ref: NodeRef<leptos::html::Div> = NodeRef::new();
    let joystick_ref: NodeRef<leptos::html::Div> = NodeRef::new();
    let status = RwSignal::new(String::from("Loading\u{2026}"));
    let debug_visible = RwSignal::new(false);
    let save_files = RwSignal::new(Vec::<DownloadableSaveFile>::new());
    let machine_handle = Rc::new(RefCell::new(None::<RuntimeHandle>));
    let machine_ready = RwSignal::new(false);
    let mobile_controls = game.settings.mobile_controls;
    let selected_plugin_ids_for_effect = selected_plugin_ids.clone();
    let selected_plugin_ids_for_shelf = selected_plugin_ids.clone();

    // `alive` is shared between the rAF loop, input handlers, and the
    // on_cleanup hook. When this component unmounts (user clicks "Library")
    // we flip it to false; the rAF loop sees it on the next tick and
    // stops scheduling itself, which lets the Rc<RefCell<Machine>> drop —
    // closing the AudioContext (via Machine::Drop → WebAudioBackend::Drop)
    // and freeing the emulator's RAM. Without this the loop keeps running
    // forever and switching games would leak a Machine per visit.
    // Uses Arc<AtomicBool> (not Rc<Cell>) because Leptos's on_cleanup
    // requires Send + Sync even in CSR mode.
    let alive: Arc<AtomicBool> = Arc::new(AtomicBool::new(true));
    let alive_for_cleanup = alive.clone();
    let pending_worker = StoredValue::new_local(Rc::new(RefCell::new(None::<Worker>)));
    let runtime_cleanup = StoredValue::new_local(machine_handle.clone());
    let audio_bootstrap = crate::emulator::prepare_audio_for_boot();
    let audio_cleanup = StoredValue::new_local(audio_bootstrap.clone());
    let input_listeners: InputListeners = Rc::new(RefCell::new(Vec::new()));
    let input_cleanup = StoredValue::new_local(input_listeners.clone());
    let local_render = Rc::new(RefCell::new(None::<BrowserRenderLoop>));
    let render_cleanup = StoredValue::new_local(local_render.clone());
    on_cleanup(move || {
        alive_for_cleanup.store(false, Ordering::Relaxed);
        audio_cleanup.try_with_value(|bootstrap| bootstrap.borrow_mut().take());
        input_cleanup.try_with_value(|listeners| listeners.borrow_mut().clear());
        render_cleanup.try_with_value(|render| render.borrow_mut().take());
        pending_worker.try_with_value(|slot| {
            if let Some(worker) = slot.borrow_mut().take() {
                cancel_systemless_worker(&worker);
            }
        });
        runtime_cleanup.try_with_value(|handle| match handle.borrow_mut().take() {
            Some(RuntimeHandle::Worker(runtime)) => runtime.stop(),
            Some(RuntimeHandle::Local(machine)) => {
                let id = machine.borrow_mut().flush_save_files();
                spawn_local(async move {
                    if let Err(error) = save_store::flush_pending_saves(&id).await {
                        crate::app::report_runtime_notice(error);
                    }
                });
            }
            None => {}
        });
    });

    let machine_handle_for_effect = machine_handle.clone();
    Effect::new(move |_| {
        let Some(canvas) = canvas_ref.get() else {
            return;
        };
        let primary_url = primary_archive_url(game);
        let fallback_url = game
            .assets
            .web_pack_path
            .map(|_| asset_path(game.assets.archive_path));
        let arrows_as_numpad = game.settings.arrows_as_numpad;
        let launch_modifiers = game.settings.launch_modifiers;
        let show_menu_bar = game.settings.show_menu_bar;
        let screen_depth = game.settings.screen_depth;
        let application_partition_size = game.settings.application_partition_size;
        let remove_paths = game.settings.remove_paths;
        let file_mappings = game.settings.file_mappings;
        let runtime_pacing = game.settings.runtime_pacing;
        let selected_plugin_ids = selected_plugin_ids_for_effect.clone();
        let alive = alive.clone();
        let audio_bootstrap = audio_bootstrap.clone();
        let input_listeners = input_listeners.clone();
        let local_render = local_render.clone();
        machine_ready.set(false);
        *machine_handle_for_effect.borrow_mut() = None;
        let machine_handle_for_task = machine_handle_for_effect.clone();
        attach_debug_toggle(&canvas, alive.clone(), debug_visible, &input_listeners);
        spawn_local_scoped_with_cancellation(async move {
            set_status(status, "Fetching game\u{2026}".into());
            let bytes = match fetch_bytes(&primary_url, |received, total| {
                set_status(status, fetch_status(received, total));
            })
            .await
            {
                Ok(b) => b,
                Err(primary_error) => match fallback_url.as_deref() {
                    Some(url) => {
                        set_status(status, "Fetching game\u{2026}".into());
                        match fetch_bytes(url, |received, total| {
                            set_status(status, fetch_status(received, total));
                        })
                        .await
                        {
                            Ok(b) => b,
                            Err(fallback_error) => {
                                set_status(status, format!(
                                    "Fetch failed: {primary_error}; fallback failed: {fallback_error}"
                                ));
                                return;
                            }
                        }
                    }
                    None => {
                        set_status(status, format!("Fetch failed: {primary_error}"));
                        return;
                    }
                },
            };
            // Component may have already unmounted during the fetch.
            if !alive.load(Ordering::Relaxed) {
                return;
            }
            let selected_plugins = selected_plugin_ids
                .iter()
                .filter_map(|id| game.settings.plugins.iter().find(|plugin| plugin.id == *id))
                .collect::<Vec<_>>();
            let plugin_files = match fetch_selected_plugins(&selected_plugins, status).await {
                Ok(files) => files,
                Err(error) => {
                    set_status(status, format!("Plugin failed: {error}"));
                    return;
                }
            };
            if !alive.load(Ordering::Relaxed) {
                return;
            }
            if let Err(error) = JsFuture::from(wait_systemless_worker_shutdown(game.id)).await {
                set_status(
                    status,
                    format!(
                        "Previous game could not finish saving: {}",
                        js_value_string(error)
                    ),
                );
                return;
            }
            if let Err(error) = save_store::flush_pending_saves(game.id).await {
                set_status(
                    status,
                    format!("Previous game could not finish saving: {error}"),
                );
                return;
            }
            if !alive.load(Ordering::Relaxed) {
                return;
            }
            if game.settings.worker {
                set_status(status, "Starting runtime worker\u{2026}".into());
                match boot_catalogue_worker(
                    &bytes,
                    &plugin_files,
                    game,
                    architecture,
                    save_files,
                    status,
                    pending_worker,
                    alive.clone(),
                    audio_bootstrap.clone(),
                    input_listeners.clone(),
                )
                .await
                {
                    Ok(runtime) => {
                        if !alive.load(Ordering::Relaxed) {
                            runtime.stop();
                            return;
                        }
                        let ready_canvas = canvas.clone();
                        let mark_runtime_ready = Box::new(move || {
                            mark_canvas_runtime_ready(
                                &ready_canvas,
                                game,
                                architecture,
                                true,
                                status,
                            );
                        });
                        *machine_handle_for_task.borrow_mut() =
                            Some(RuntimeHandle::Worker(runtime.clone()));
                        machine_ready.set(true);
                        attach_input(
                            &canvas,
                            RuntimeHandle::Worker(runtime.clone()),
                            alive.clone(),
                            game.settings.key_mappings,
                            &input_listeners,
                        );
                        if mobile_controls.enabled {
                            if let (Some(controls), Some(joystick)) =
                                (controls_ref.get(), joystick_ref.get())
                            {
                                attach_mobile_controls(
                                    &controls,
                                    &joystick,
                                    mobile_controls,
                                    RuntimeHandle::Worker(runtime.clone()),
                                    alive.clone(),
                                    &input_listeners,
                                );
                            }
                        }
                        start_worker_render_loop(
                            canvas,
                            runtime,
                            alive,
                            debug_visible,
                            mark_runtime_ready,
                        );
                        return;
                    }
                    Err(error) => {
                        if !alive.load(Ordering::Relaxed) {
                            return;
                        }
                        let _ = canvas.set_attribute("data-runtime-fallback", &error);
                        set_status(status, format!(
                            "Runtime worker unavailable ({error}); starting compatible runtime\u{2026}"
                        ));
                    }
                }
            }
            set_status(
                status,
                format!("Preparing game data ({} KB)\u{2026}", bytes.len() / 1024),
            );
            yield_to_browser_frame().await;
            if !alive.load(Ordering::Relaxed) {
                return;
            }
            let mut machine = match Machine::new_with_progress(
                game.id,
                &bytes,
                &plugin_files,
                architecture,
                launch_modifiers,
                show_menu_bar,
                screen_depth,
                application_partition_size,
                remove_paths,
                file_mappings,
                runtime_pacing,
                &audio_bootstrap,
                |progress| set_status(status, boot_progress_status(progress)),
            )
            .await
            {
                Ok(m) => m,
                Err(e) => {
                    set_status(status, format!("Boot failed: {e}"));
                    return;
                }
            };
            machine.set_arrows_as_numpad(arrows_as_numpad);
            if !alive.load(Ordering::Relaxed) {
                return;
            }
            let ready_canvas = canvas.clone();
            let mark_runtime_ready = Box::new(move || {
                mark_canvas_runtime_ready(&ready_canvas, game, architecture, false, status);
            });
            let machine = Rc::new(RefCell::new(machine));
            *machine_handle_for_task.borrow_mut() = Some(RuntimeHandle::Local(machine.clone()));
            machine_ready.set(true);
            attach_input(
                &canvas,
                RuntimeHandle::Local(machine.clone()),
                alive.clone(),
                game.settings.key_mappings,
                &input_listeners,
            );
            if mobile_controls.enabled {
                if let (Some(controls), Some(joystick)) = (controls_ref.get(), joystick_ref.get()) {
                    attach_mobile_controls(
                        &controls,
                        &joystick,
                        mobile_controls,
                        RuntimeHandle::Local(machine.clone()),
                        alive.clone(),
                        &input_listeners,
                    );
                }
            }
            start_render_loop(
                canvas,
                machine,
                alive,
                debug_visible,
                Some(mark_runtime_ready),
                save_files,
                status,
                local_render,
            );
        });
    });

    let controls_view = if mobile_controls.enabled {
        let button_panel_view = mobile_button_panel_view(mobile_controls);
        view! {
            <div
                class="mobile-controls"
                node_ref=controls_ref
                data-mobile-controls="true"
                aria-hidden="true"
            >
                <div
                    class="mobile-joystick"
                    node_ref=joystick_ref
                    data-mobile-joystick="true"
                >
                    <div class="mobile-joystick__ring">
                        <div class="mobile-joystick__thumb"></div>
                    </div>
                </div>
                {button_panel_view}
            </div>
            <div class="mobile-rotate-hint" aria-hidden="true">
                <span class="mobile-rotate-hint__icon"></span>
                <span>"Rotate for fullscreen controls"</span>
            </div>
        }
        .into_any()
    } else {
        view! {}.into_any()
    };

    view! {
        <div class="game-canvas-wrap">
            <canvas
                class="game-canvas"
                node_ref=canvas_ref
                width="640"
                height="480"
                tabindex="0"
                data-game-id=game.id
                data-architecture=architecture.key()
                data-arrows-as-numpad=bool_attr(game.settings.arrows_as_numpad)
                data-show-menu-bar=bool_attr(game.settings.show_menu_bar)
                data-app-partition-size=game.settings.application_partition_size.map(|bytes| bytes.to_string())
            />
            <div class="game-status" class:hidden={move || status.get().is_empty()}>
                {move || status.get()}
            </div>
            {controls_view}
        </div>
        <GameFilesDialog
            game=game
            selected_plugin_ids=selected_plugin_ids_for_shelf
            selected_plugins_signal=selected_plugins_signal
            files=save_files
            machine=machine_handle
            machine_ready=machine_ready
        />
    }
}

fn mobile_button_panel_view(mobile_controls: MobileControls) -> AnyView {
    if mobile_controls.button_groups.is_empty() {
        return view! {
            <div class="mobile-button-panel" data-mobile-button-panel="true">
                {mobile_button_grid_view(mobile_controls.buttons, true, 0)}
            </div>
        }
        .into_any();
    }

    let groups = mobile_controls.button_groups;
    view! {
        <div class="mobile-button-panel" data-mobile-button-panel="true">
            <div class="mobile-button-tabs">
                {groups.iter().enumerate().map(|(index, group)| {
                    let class = if index == 0 {
                        "mobile-button-tab is-active"
                    } else {
                        "mobile-button-tab"
                    };
                    let pressed = if index == 0 { "true" } else { "false" };
                    view! {
                        <button
                            class=class
                            type="button"
                            data-mobile-control-group-tab=index.to_string()
                            aria-pressed=pressed
                        >
                            {group.label}
                        </button>
                    }
                }).collect_view()}
            </div>
            <div class="mobile-button-group-stack">
                {groups.iter().enumerate().map(|(index, group)| {
                    mobile_button_grid_view(group.buttons, index == 0, index)
                }).collect_view()}
            </div>
        </div>
    }
    .into_any()
}

fn mobile_button_grid_view(
    buttons: &'static [MobileControlButton],
    active: bool,
    group_index: usize,
) -> AnyView {
    let class = if active {
        "mobile-buttons is-active"
    } else {
        "mobile-buttons"
    };
    view! {
        <div class=class data-mobile-control-group=group_index.to_string()>
            {buttons.iter().map(|button| {
                view! {
                    <button
                        class="mobile-button"
                        type="button"
                        data-mobile-control-key=button.key
                        aria-label=button.label
                    >
                        {button.label}
                    </button>
                }
            }).collect_view()}
        </div>
    }
    .into_any()
}

const GAME_INFO_TABS: [(&str, &str); 3] = [
    ("keys", "Mapped keys"),
    ("license", "License"),
    ("issues", "Open an issue"),
];

#[component]
fn GameFilesDialog(
    game: &'static Game,
    selected_plugin_ids: Vec<&'static str>,
    selected_plugins_signal: RwSignal<Vec<&'static str>>,
    files: RwSignal<Vec<DownloadableSaveFile>>,
    machine: Rc<RefCell<Option<RuntimeHandle>>>,
    machine_ready: RwSignal<bool>,
) -> impl IntoView {
    let active_tab = RwSignal::new("about");
    let mut tabs = vec![("about", "About")];
    if !game.settings.plugins.is_empty() {
        tabs.push(("plugins", "Plugins"));
    }
    if game.approved {
        tabs.push(("saved", "Saved files"));
    }
    tabs.extend(GAME_INFO_TABS);
    let keyboard_tabs = tabs.clone();
    let installable_count = game
        .settings
        .plugins
        .iter()
        .filter(|p| !p.install_assets.is_empty())
        .count();
    view! {
        <section class="game-files-dialog" aria-label="Game information and files">
            <div class="game-files-dialog__tabs" role="tablist" aria-label="Game information and files"
                on:keydown=move |event: KeyboardEvent| {
                    let current = keyboard_tabs.iter().position(|(id, _)| *id == active_tab.get()).unwrap_or(0);
                    let next = match event.key().as_str() {
                        "ArrowRight" => (current + 1) % keyboard_tabs.len(),
                        "ArrowLeft" => (current + keyboard_tabs.len() - 1) % keyboard_tabs.len(),
                        "Home" => 0,
                        "End" => keyboard_tabs.len() - 1,
                        _ => return,
                    };
                    event.prevent_default();
                    event.stop_propagation();
                    let id = keyboard_tabs[next].0;
                    active_tab.set(id);
                    if let Some(element) = web_sys::window().and_then(|w| w.document())
                        .and_then(|d| d.get_element_by_id(&format!("{}-tab-{id}", game.id)))
                        .and_then(|e| e.dyn_into::<HtmlElement>().ok()) {
                        let _ = element.focus();
                    }
                }
            >
                {tabs.iter().map(|&(id, label)| view! {
                    <button type="button" role="tab" id=format!("{}-tab-{id}", game.id)
                        aria-controls=format!("{}-panel-{id}", game.id)
                        aria-selected=move || bool_attr(active_tab.get() == id)
                        tabindex=move || if active_tab.get() == id { "0" } else { "-1" }
                        class="game-files-tab" class:game-files-tab--active=move || active_tab.get() == id
                        on:click=move |_| active_tab.set(id)>
                        {label}
                        {if id == "saved" { view! { <span class="game-files-tab__count">{move || files.get().len()}</span> }.into_any() }
                        else if id == "plugins" { view! { <span class="game-files-tab__count">{selected_plugin_ids.len()}</span> }.into_any() }
                        else { view! {}.into_any() }}
                    </button>
                }).collect_view()}
            </div>
            {tabs.into_iter().map(|(id, _)| {
                let content = match id {
                    "plugins" => view! { <PluginPanel game=game selected_plugin_ids=selected_plugin_ids.clone()
                        selected_plugins_signal=selected_plugins_signal installable_count=installable_count/> }.into_any(),
                    "saved" => view! { <SavePanel files=files machine=machine.clone() machine_ready=machine_ready/> }.into_any(),
                    _ => view! { <super::game_info::GameInfoContent game=game tab=id/> }.into_any(),
                };
                view! {
                    <div class="game-files-panel" class:hidden=move || active_tab.get() != id
                        role="tabpanel" id=format!("{}-panel-{id}", game.id)
                        aria-labelledby=format!("{}-tab-{id}", game.id) tabindex="0">
                        {content}
                    </div>
                }
            }).collect_view()}
        </section>
    }
}

#[component]
pub fn UnavailableGameScreen(game: &'static Game) -> impl IntoView {
    view! {
        <p>"Launching is not enabled for this catalogue entry."</p>
        <GameFilesDialog game=game selected_plugin_ids=Vec::new()
            selected_plugins_signal=RwSignal::new(Vec::new())
            files=RwSignal::new(Vec::new()) machine=Rc::new(RefCell::new(None))
            machine_ready=RwSignal::new(false)/>
    }
}

#[component]
fn PluginPanel(
    game: &'static Game,
    selected_plugin_ids: Vec<&'static str>,
    selected_plugins_signal: RwSignal<Vec<&'static str>>,
    installable_count: usize,
) -> impl IntoView {
    let selected_plugins = game
        .settings
        .plugins
        .iter()
        .filter(|plugin| selected_plugin_ids.contains(&plugin.id))
        .collect::<Vec<_>>();
    let selected_count = selected_plugins.len();

    view! {
        <div class="plugin-panel">
            <div class="plugin-panel__summary">
                <span class="plugin-panel__count">
                    {format!("{selected_count} selected · {installable_count} installable · {} total", game.settings.plugins.len())}
                </span>
                <button
                    class="plugin-clear"
                    class:hidden=move || selected_count == 0
                    type="button"
                    on:click=move |_| {
                        if !selected_plugins_signal.get_untracked().is_empty() {
                            if game.approved {
                                crate::emulator::begin_audio_from_user_gesture();
                            }
                            selected_plugins_signal.set(Vec::new());
                        }
                    }
                >
                    "Disable all"
                </button>
            </div>
            <div class="plugin-selected" class:hidden=move || selected_count == 0>
                <div class="plugin-section-title">"Selected"</div>
                <div class="plugin-selected-list">
                    {selected_plugins.iter().map(|plugin| {
                        let plugin = **plugin;
                        view! {
                            <div class="plugin-selected-row">
                                <span class="plugin-selected-row__label" title=plugin.label>{plugin.label}</span>
                                <span class="plugin-selected-row__meta">
                                    {format!("{} file{}", plugin.install_assets.len(), if plugin.install_assets.len() == 1 { "" } else { "s" })}
                                </span>
                                <button
                                    class="plugin-disable"
                                    type="button"
                                    on:click=move |_| set_plugin_selected(
                                        game,
                                        selected_plugins_signal,
                                        plugin.id,
                                        false,
                                    )
                                >
                                    "Disable"
                                </button>
                            </div>
                        }
                    }).collect_view()}
                </div>
            </div>
            <div class="plugin-section-title">"All plugins"</div>
            <div class="plugin-list">
                {game.settings.plugins.iter().map(|plugin| {
                    let is_selected = selected_plugin_ids.contains(&plugin.id);
                    let is_installable = game.approved && !plugin.install_assets.is_empty();
                    let status_text = if is_installable {
                        format!("Installable · {} file{}", plugin.install_assets.len(), if plugin.install_assets.len() == 1 { "" } else { "s" })
                    } else {
                        "Download only".to_string()
                    };
                    let row_class = if is_installable {
                        "plugin-row"
                    } else {
                        "plugin-row plugin-row--disabled"
                    };
                    view! {
                        <div class=row_class>
                            <label class="plugin-choice">
                                <input
                                    class="plugin-checkbox"
                                    type="checkbox"
                                    checked=is_selected
                                    disabled=!is_installable
                                    on:change=move |event| {
                                        let checked = event
                                            .target()
                                            .and_then(|target| target.dyn_into::<HtmlInputElement>().ok())
                                            .map(|input| input.checked())
                                            .unwrap_or(false);
                                        set_plugin_selected(
                                            game,
                                            selected_plugins_signal,
                                            plugin.id,
                                            checked,
                                        );
                                    }
                                />
                                <span class="plugin-row__body">
                                    <span class="plugin-row__title">{plugin.label}</span>
                                    <span class="plugin-row__meta">
                                        {if plugin.size_bytes == 0 { status_text.clone() } else { format!("{status_text} · {}", format_plugin_size(plugin.size_bytes)) }}
                                    </span>
                                    <span class="plugin-row__description">{plugin.description}</span>
                                </span>
                            </label>
                            <a
                                class="plugin-download"
                                href=plugin.download_path
                                target="_blank"
                                rel="noopener noreferrer"
                                download=plugin.download_name
                            >
                                "Download"
                            </a>
                        </div>
                    }
                }).collect_view()}
            </div>
        </div>
    }
    .into_any()
}

fn format_plugin_size(size_bytes: u32) -> String {
    if size_bytes >= 1024 * 1024 {
        format!("{:.1} MB", size_bytes as f64 / (1024.0 * 1024.0))
    } else if size_bytes >= 1024 {
        format!("{:.1} KB", size_bytes as f64 / 1024.0)
    } else {
        format!("{size_bytes} B")
    }
}

#[component]
fn SavePanel(
    files: RwSignal<Vec<DownloadableSaveFile>>,
    machine: Rc<RefCell<Option<RuntimeHandle>>>,
    machine_ready: RwSignal<bool>,
) -> impl IntoView {
    let input_ref: NodeRef<leptos::html::Input> = NodeRef::new();
    let import_error = RwSignal::new(String::new());
    let import_machine = machine.clone();
    let action_machine = machine.clone();

    view! {
        <div class="save-panel">
            <div class="save-panel__header">
                <span class="save-panel__hint">"Import or download MacBinary save files."</span>
                <span class="save-panel__actions">
                    <button
                        class="save-import"
                        type="button"
                        disabled=move || !machine_ready.get()
                        on:click=move |_| {
                            if let Some(input) = input_ref.get() {
                                input.click();
                            }
                        }
                    >
                        "Import"
                    </button>
                    <input
                        class="save-import__input"
                        node_ref=input_ref
                        type="file"
                        accept=".bin,.macbin,application/x-macbinary,application/octet-stream"
                        on:change=move |event| {
                            handle_save_import(event, import_machine.clone(), files, import_error)
                        }
                    />
                </span>
            </div>
            <div class="save-panel__message" class:hidden=move || import_error.get().is_empty()>
                {move || import_error.get()}
            </div>
            <div class="save-empty" class:hidden=move || !files.get().is_empty()>
                "No saved files"
            </div>
            <div
                class="save-list"
                class:hidden=move || files.get().is_empty()
                on:click=move |event| {
                    handle_save_file_action(event, action_machine.clone(), files, import_error)
                }
            >
                {move || files.get().into_iter().map(|file| {
                    let size = format_save_size(file.data_len, file.resource_len);
                    let label = format!("{} · {} ", file.name, size);
                    view! {
                        <div class="save-file">
                            <span class="save-file__name">{label}</span>
                            <span class="save-file__actions">
                                <button
                                    class="save-file__action"
                                    type="button"
                                    title=format!("Download {} ({})", file.name, size)
                                    aria-label=format!("Download {} ({})", file.name, size)
                                    data-save-action="download"
                                    data-save-path=file.path.clone()
                                >
                                    "Download"
                                </button>
                                <button
                                    class="save-file__action save-file__action--delete"
                                    type="button"
                                    title=format!("Remove saved file {}", file.name)
                                    aria-label=format!("Remove saved file {}", file.name)
                                    data-save-action="remove"
                                    data-save-path=file.path.clone()
                                >
                                    "Remove"
                                </button>
                            </span>
                        </div>
                    }
                }).collect_view()}
            </div>
        </div>
    }
}

fn handle_save_file_action(
    event: MouseEvent,
    machine: Rc<RefCell<Option<RuntimeHandle>>>,
    files: RwSignal<Vec<DownloadableSaveFile>>,
    import_error: RwSignal<String>,
) {
    let Some((action, path)) = save_file_action_target(&event) else {
        return;
    };

    event.prevent_default();
    match action.as_str() {
        "download" => {
            if let Some(file) = files
                .get_untracked()
                .into_iter()
                .find(|file| file.path == path)
            {
                let _ = save_store::download_save_file(&file);
            }
        }
        "remove" => {
            import_error.set(String::new());
            let result = machine
                .borrow()
                .clone()
                .ok_or_else(|| "Game is not ready".to_string())
                .and_then(|machine| match machine {
                    RuntimeHandle::Local(machine) => {
                        let mut machine = machine.borrow_mut();
                        machine.delete_save_file(&path)?;
                        Ok(machine.save_files().to_vec())
                    }
                    RuntimeHandle::Worker(worker) => {
                        worker.delete_save(&path)?;
                        Ok(files.get_untracked())
                    }
                });
            match result {
                Ok(updated_files) => files.set(updated_files),
                Err(message) => import_error.set(format!("Remove failed: {message}")),
            }
        }
        _ => {}
    }
}

fn save_file_action_target(event: &MouseEvent) -> Option<(String, String)> {
    let mut element = event.target()?.dyn_into::<Element>().ok();
    while let Some(current) = element {
        if let (Some(action), Some(path)) = (
            current.get_attribute("data-save-action"),
            current.get_attribute("data-save-path"),
        ) {
            return Some((action, path));
        }
        element = current.parent_element();
    }
    None
}

fn handle_save_import(
    event: Event,
    machine: Rc<RefCell<Option<RuntimeHandle>>>,
    files: RwSignal<Vec<DownloadableSaveFile>>,
    import_error: RwSignal<String>,
) {
    import_error.set(String::new());
    let Some(input) = event
        .target()
        .and_then(|target| target.dyn_into::<HtmlInputElement>().ok())
    else {
        return;
    };
    let Some(file) = input.files().and_then(|files| files.item(0)) else {
        return;
    };

    spawn_local(async move {
        let result = async {
            let buffer = JsFuture::from(file.array_buffer())
                .await
                .map_err(js_value_string)?;
            let bytes = Uint8Array::new(&buffer).to_vec();
            let Some(machine) = machine.borrow().clone() else {
                return Err("Game is not ready".to_string());
            };
            match machine {
                RuntimeHandle::Local(machine) => {
                    let mut machine = machine.borrow_mut();
                    machine.import_save_file_bytes(&bytes)?;
                    Ok(machine.save_files().to_vec())
                }
                RuntimeHandle::Worker(worker) => {
                    worker.import_save(&bytes)?;
                    Ok(files.get_untracked())
                }
            }
        }
        .await;

        input.set_value("");
        match result {
            Ok(updated_files) => files.set(updated_files),
            Err(message) => import_error.set(format!("Import failed: {message}")),
        }
    });
}

fn js_value_string(value: JsValue) -> String {
    value.as_string().unwrap_or_else(|| format!("{value:?}"))
}

fn format_save_size(data_len: usize, resource_len: usize) -> String {
    let total = data_len.saturating_add(resource_len);
    if total < 1024 {
        format!("{total} B")
    } else if total < 1024 * 1024 {
        format!("{:.1} KB", total as f64 / 1024.0)
    } else {
        format!("{:.1} MB", total as f64 / (1024.0 * 1024.0))
    }
}

fn bool_attr(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn fetch_status(received: usize, total: Option<usize>) -> String {
    match total.filter(|total| *total > 0) {
        Some(total) => {
            let percent = (received as f64 * 100.0 / total as f64).clamp(0.0, 100.0);
            format!("Fetching game ({percent:.0}%)\u{2026}")
        }
        None if received > 0 => format!("Fetching game ({} KB)\u{2026}", received / 1024),
        None => "Fetching game\u{2026}".into(),
    }
}

async fn fetch_selected_plugins(
    plugins: &[&'static GamePlugin],
    status: RwSignal<String>,
) -> Result<Vec<PluginFile>, String> {
    let mut files = Vec::new();
    for (index, plugin) in plugins.iter().enumerate() {
        if plugin.install_assets.is_empty() {
            return Err(format!("{} is only available as a download", plugin.label));
        }
        for (asset_index, install_asset) in plugin.install_assets.iter().enumerate() {
            let url = asset_path(install_asset.asset_path);
            set_status(
                status,
                format!(
                    "Fetching plugin {}/{}: {} ({}/{})\u{2026}",
                    index + 1,
                    plugins.len(),
                    plugin.label,
                    asset_index + 1,
                    plugin.install_assets.len()
                ),
            );
            let bytes = fetch_bytes(&url, |received, total| {
                set_status(status, plugin_fetch_status(plugin.label, received, total));
            })
            .await?;
            let file = save_store::decode_macbinary_save_file("", &bytes).map_err(|error| {
                format!("{} is not a valid MacBinary plugin: {error}", plugin.label)
            })?;
            files.push(PluginFile {
                mount_path: install_asset.mount_path.to_string(),
                file,
            });
        }
    }

    Ok(files)
}

fn plugin_fetch_status(label: &str, received: usize, total: Option<usize>) -> String {
    match total.filter(|total| *total > 0) {
        Some(total) => {
            let percent = (received as f64 * 100.0 / total as f64).clamp(0.0, 100.0);
            format!("Fetching plugin: {label} ({percent:.0}%)\u{2026}")
        }
        None if received > 0 => {
            format!("Fetching plugin: {label} ({} KB)\u{2026}", received / 1024)
        }
        None => format!("Fetching plugin: {label}\u{2026}"),
    }
}

fn mark_canvas_runtime_ready(
    canvas: &HtmlCanvasElement,
    game: &Game,
    architecture: GameArchitecture,
    worker: bool,
    status: RwSignal<String>,
) {
    let settings = &game.settings;
    for (name, value) in [
        ("data-runtime-game-id", game.id.to_string()),
        ("data-runtime-worker", bool_attr(worker).to_string()),
        ("data-runtime-architecture", architecture.key().to_string()),
        (
            "data-runtime-arrows-as-numpad",
            bool_attr(settings.arrows_as_numpad).to_string(),
        ),
        (
            "data-runtime-show-menu-bar",
            bool_attr(settings.show_menu_bar).to_string(),
        ),
        (
            "data-runtime-cpu-mhz",
            settings.runtime_pacing.cpu_mhz.to_string(),
        ),
        (
            "data-runtime-max-ticks-per-paint",
            settings.runtime_pacing.max_ticks_per_paint.to_string(),
        ),
    ] {
        let _ = canvas.set_attribute(name, &value);
    }
    if let Some(bytes) = settings.application_partition_size {
        let _ = canvas.set_attribute("data-runtime-app-partition-size", &bytes.to_string());
    }
    set_status(status, String::new());
}

fn set_status(status: RwSignal<String>, value: String) {
    // Fetch/boot futures may finish after their component has been disposed.
    let _ = status.try_set(value);
}

fn boot_progress_status(progress: BootProgress) -> String {
    match progress {
        BootProgress::MountingArchive {
            loaded_bytes,
            total_bytes,
        } => match total_bytes {
            0 => "Mounting game data\u{2026}".into(),
            total => {
                let percent = (loaded_bytes as f64 * 100.0 / total as f64).clamp(0.0, 100.0);
                format!("Mounting game data ({percent:.0}%)\u{2026}")
            }
        },
        BootProgress::LoadingExecutable => "Loading executable\u{2026}".into(),
        BootProgress::StartingRuntime => "Starting Mac runtime\u{2026}".into(),
        BootProgress::PreparingAudio => "Preparing audio\u{2026}".into(),
    }
}

struct OwnedArchiveFetch(js_sys::Promise);

impl Drop for OwnedArchiveFetch {
    fn drop(&mut self) {
        crate::browser_bridge::cancel_archive_fetch(&self.0);
    }
}

struct FetchAbortGuard(web_sys::AbortController);

impl Drop for FetchAbortGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn fetch_bytes<F>(url: &str, mut on_progress: F) -> Result<Vec<u8>, String>
where
    F: FnMut(usize, Option<usize>),
{
    if let Some(prefetched) = prefetched_archive_promise(url) {
        let _fetch = OwnedArchiveFetch(prefetched.clone());
        if let Ok(buffer) = JsFuture::from(prefetched).await {
            return uint8_array_to_vec_chunked(&Uint8Array::new(&buffer), &mut on_progress).await;
        }
    }

    match fetch_bytes_streaming(url, &mut on_progress).await {
        Ok(bytes) => return Ok(bytes),
        Err(stream_error) => {
            if let Ok(promise) = fetch_archive_in_worker(url) {
                let _fetch = OwnedArchiveFetch(promise.clone());
                if let Ok(buffer) = JsFuture::from(promise).await {
                    return uint8_array_to_vec_chunked(&Uint8Array::new(&buffer), &mut on_progress)
                        .await;
                }
            }
            Err(stream_error)
        }
    }
}

fn prefetched_archive_promise(url: &str) -> Option<js_sys::Promise> {
    let value = prefetched_archive(url);
    if value.is_null() || value.is_undefined() {
        None
    } else {
        value.dyn_into::<js_sys::Promise>().ok()
    }
}

fn primary_archive_url(game: &Game) -> String {
    asset_path(
        game.assets
            .web_pack_path
            .unwrap_or(game.assets.archive_path),
    )
}

async fn fetch_bytes_streaming<F>(url: &str, on_progress: &mut F) -> Result<Vec<u8>, String>
where
    F: FnMut(usize, Option<usize>),
{
    let abort = FetchAbortGuard(
        web_sys::AbortController::new()
            .map_err(|error| js_error_string("fetch cancellation", error))?,
    );
    let resp = gloo_net::http::Request::get(url)
        .abort_signal(Some(&abort.0.signal()))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    if resp
        .headers()
        .get("content-type")
        .is_some_and(|value| value.to_ascii_lowercase().contains("text/html"))
    {
        return Err("archive URL returned HTML instead of game data".to_string());
    }

    let Some(body) = resp.body() else {
        let bytes = resp.binary().await.map_err(|e| e.to_string())?;
        on_progress(bytes.len(), Some(bytes.len()));
        return Ok(bytes);
    };
    let reader =
        ReadableStreamDefaultReader::new(&body).map_err(|e| js_error_string("stream", e))?;
    let content_len = resp
        .headers()
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok());
    let mut bytes = Vec::with_capacity(content_len.unwrap_or(0));

    loop {
        let read = JsFuture::from(reader.read())
            .await
            .map_err(|e| js_error_string("stream read", e))?;
        let read: ReadableStreamReadResult = read.unchecked_into();
        if ReadableStreamReadResult::get_done(&read).unwrap_or(false) {
            break;
        }
        let value = ReadableStreamReadResult::get_value(&read);
        if value.is_null() || value.is_undefined() {
            continue;
        }
        let chunk: Uint8Array = value.unchecked_into();
        append_uint8_array_chunked(&mut bytes, &chunk, content_len, on_progress).await;
    }
    reader.release_lock();

    Ok(bytes)
}

async fn uint8_array_to_vec_chunked<F>(
    array: &Uint8Array,
    on_progress: &mut F,
) -> Result<Vec<u8>, String>
where
    F: FnMut(usize, Option<usize>),
{
    let mut bytes = Vec::with_capacity(array.length() as usize);
    let total = Some(array.length() as usize);
    append_uint8_array_chunked(&mut bytes, array, total, on_progress).await;
    Ok(bytes)
}

async fn append_uint8_array_chunked<F>(
    out: &mut Vec<u8>,
    chunk: &Uint8Array,
    total: Option<usize>,
    on_progress: &mut F,
) where
    F: FnMut(usize, Option<usize>),
{
    let mut offset = 0;
    let len = chunk.length();
    let mut copied_since_frame_yield = 0;
    while offset < len {
        let end = offset.saturating_add(FETCH_COPY_YIELD_BYTES).min(len);
        let slice = chunk.subarray(offset, end);
        let old_len = out.len();
        out.resize(old_len + slice.length() as usize, 0);
        slice.copy_to(&mut out[old_len..]);
        copied_since_frame_yield += slice.length();
        offset = end;
        on_progress(out.len(), total);
        if offset < len {
            if copied_since_frame_yield >= FETCH_COPY_FRAME_YIELD_BYTES {
                copied_since_frame_yield = 0;
                yield_to_browser_frame().await;
            } else {
                yield_to_browser_task().await;
            }
        }
    }
}

async fn yield_to_browser_frame() {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let Some(window) = web_sys::window() else {
            let _ = resolve.call0(&JsValue::UNDEFINED);
            return;
        };
        let window_for_timeout = window.clone();
        let resolve_for_raf = resolve.clone();
        let cb = Closure::once(move || {
            let resolve_for_timeout = resolve_for_raf.clone();
            let timeout_cb = Closure::once(move || {
                let _ = resolve_for_timeout.call0(&JsValue::UNDEFINED);
            });
            if window_for_timeout
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    timeout_cb.as_ref().unchecked_ref(),
                    0,
                )
                .is_ok()
            {
                timeout_cb.forget();
            } else {
                let _ = resolve_for_raf.call0(&JsValue::UNDEFINED);
            }
        });
        if window
            .request_animation_frame(cb.as_ref().unchecked_ref())
            .is_ok()
        {
            cb.forget();
        } else {
            let _ = resolve.call0(&JsValue::UNDEFINED);
        }
    });
    let _ = JsFuture::from(promise).await;
}

async fn yield_to_browser_task() {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let Some(window) = web_sys::window() else {
            let _ = resolve.call0(&JsValue::UNDEFINED);
            return;
        };
        let resolve_for_timeout = resolve.clone();
        let timeout_cb = Closure::once(move || {
            let _ = resolve_for_timeout.call0(&JsValue::UNDEFINED);
        });
        if window
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                timeout_cb.as_ref().unchecked_ref(),
                0,
            )
            .is_ok()
        {
            timeout_cb.forget();
        } else {
            let _ = resolve.call0(&JsValue::UNDEFINED);
        }
    });
    let _ = JsFuture::from(promise).await;
}

fn js_error_string(context: &str, value: JsValue) -> String {
    if let Some(message) = value.as_string() {
        format!("{context}: {message}")
    } else {
        format!("{context}: {value:?}")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn issue_actions_share_one_information_tab() {
        assert_eq!(
            super::GAME_INFO_TABS,
            [
                ("keys", "Mapped keys"),
                ("license", "License"),
                ("issues", "Open an issue"),
            ]
        );
    }

    #[test]
    fn catalogue_keyboard_mappings_apply_and_can_suppress_keys() {
        let mappings = [("ArrowUp", "w"), ("Space", "None")];
        assert_eq!(
            super::mapped_key("ArrowUp", "ArrowUp", &mappings),
            super::mobile_key_code("w")
        );
        assert_eq!(super::mapped_key(" ", "Space", &mappings), None);
        assert_eq!(
            super::mapped_key("Enter", "Enter", &mappings),
            super::map_key("Enter", "Enter")
        );
    }

    #[test]
    fn architecture_selector_reuses_homepage_toggle_classes() {
        assert_eq!(
            super::architecture_selector_class(true),
            "cli-platform__option cli-platform__option--active"
        );
        assert_eq!(
            super::architecture_selector_class(false),
            "cli-platform__option"
        );
    }

    #[test]
    fn retained_text_resolution_does_not_move_guest_pointer_coordinates() {
        for scale in 1..=4 {
            for css in [(800.0, 600.0), (640.0, 480.0), (1000.0, 750.0)] {
                assert_eq!(
                    super::logical_pointer_position(
                        (css.0 * 0.75, css.1 * 0.25),
                        css,
                        (800 * scale, 600 * scale),
                        scale as f64,
                    ),
                    (150, 600)
                );
            }
        }
    }

    #[test]
    fn slow_worker_services_latest_missed_refresh_without_another_raf() {
        let mut requests = super::WorkerFrameRequests::default();
        assert_eq!(requests.request("first"), Some("first"));
        assert_eq!(requests.request("stale scale"), None);
        assert_eq!(requests.request("latest scale"), None);
        assert_eq!(requests.complete(true), Some("latest scale"));
        assert_eq!(requests.request("next"), None);
        assert_eq!(requests.complete(true), Some("next"));
        assert_eq!(requests.complete(true), None);
    }

    #[test]
    fn fast_worker_waits_for_display_demand_and_halt_discards_pending_work() {
        let mut requests = super::WorkerFrameRequests::default();
        assert_eq!(requests.request(1), Some(1));
        assert_eq!(requests.complete(true), None);
        assert_eq!(requests.request(2), Some(2));
        assert_eq!(requests.request(3), None);
        assert_eq!(requests.complete(false), None);
        assert!(requests.pending.is_none());
        assert!(!requests.in_flight);
    }

    #[test]
    fn catchup_frames_keep_only_the_latest_rendering_backend() {
        let mut state = super::WorkerFrameState {
            save_error: None,
            output_scale: 1,
            frame: None,
            js_frame: None,
            gpu_frame: None,
            owned_frame: None,
            running: true,
            requests: super::WorkerFrameRequests::default(),
        };
        super::replace_worker_visual_frame(
            &mut state,
            super::WorkerVisualFrame::Software(1, 1, vec![0; 4]),
        );
        super::replace_worker_visual_frame(
            &mut state,
            super::WorkerVisualFrame::Gpu(wasm_bindgen::JsValue::NULL),
        );
        assert!(matches!(
            super::take_worker_visual_frame(&mut state),
            Some(super::WorkerVisualFrame::Gpu(_))
        ));
        assert!(super::take_worker_visual_frame(&mut state).is_none());
        super::replace_worker_visual_frame(
            &mut state,
            super::WorkerVisualFrame::Gpu(wasm_bindgen::JsValue::NULL),
        );
        super::replace_worker_visual_frame(
            &mut state,
            super::WorkerVisualFrame::Software(1, 1, vec![255; 4]),
        );
        assert!(matches!(
            super::take_worker_visual_frame(&mut state),
            Some(super::WorkerVisualFrame::Software(_, _, _))
        ));
        assert!(super::take_worker_visual_frame(&mut state).is_none());
    }

    #[test]
    fn indexed_and_rgba_catchup_frames_replace_each_other() {
        let mut state = super::WorkerFrameState {
            save_error: None,
            output_scale: 1,
            frame: None,
            js_frame: None,
            gpu_frame: None,
            owned_frame: None,
            running: true,
            requests: super::WorkerFrameRequests::default(),
        };
        super::replace_worker_visual_frame(
            &mut state,
            super::WorkerVisualFrame::Software(1, 1, vec![0; 4]),
        );
        super::replace_worker_visual_frame(
            &mut state,
            super::WorkerVisualFrame::Owned(wasm_bindgen::JsValue::NULL),
        );
        assert!(state.frame.is_none());
        assert!(matches!(
            super::take_worker_visual_frame(&mut state),
            Some(super::WorkerVisualFrame::Owned(_))
        ));
        super::replace_worker_visual_frame(
            &mut state,
            super::WorkerVisualFrame::Owned(wasm_bindgen::JsValue::NULL),
        );
        super::replace_worker_visual_frame(
            &mut state,
            super::WorkerVisualFrame::Software(1, 1, vec![255; 4]),
        );
        assert!(state.owned_frame.is_none());
        assert!(matches!(
            super::take_worker_visual_frame(&mut state),
            Some(super::WorkerVisualFrame::Software(_, _, _))
        ));
        assert!(super::take_worker_visual_frame(&mut state).is_none());
    }

    #[test]
    fn worker_visual_frame_uses_software_frame_without_a_second_borrow() {
        let mut state = super::WorkerFrameState {
            save_error: None,
            output_scale: 1,
            frame: Some((640, 480, vec![1, 2, 3, 4])),
            js_frame: None,
            gpu_frame: None,
            owned_frame: None,
            running: true,
            requests: super::WorkerFrameRequests::default(),
        };

        let frame = super::take_worker_visual_frame(&mut state);
        match frame {
            Some(super::WorkerVisualFrame::Software(width, height, pixels)) => {
                assert_eq!((width, height), (640, 480));
                assert_eq!(pixels, vec![1, 2, 3, 4]);
            }
            _ => panic!("software frame should be selected when no GPU frame is ready"),
        }
        assert!(state.frame.is_none());
    }

    #[test]
    fn classic_mac_letter_and_symbol_controls_are_mapped() {
        assert_eq!(super::map_key("t", "KeyT"), Some((0x11, b't')));
        assert_eq!(super::map_key("m", "KeyM"), Some((0x2E, b'm')));
        assert_eq!(super::map_key("=", "Equal"), Some((0x18, b'=')));
        assert_eq!(super::map_key("[", "BracketLeft"), Some((0x21, b'[')));
        assert_eq!(super::map_key("]", "BracketRight"), Some((0x1E, b']')));
        assert_eq!(super::map_key("\\", "Backslash"), Some((0x2A, b'\\')));
    }

    #[test]
    fn arrows_remain_arrow_key_codes_for_ev_style_controls() {
        assert_eq!(super::map_key("ArrowLeft", "ArrowLeft"), Some((0x7B, 28)));
        assert_eq!(super::map_key("ArrowRight", "ArrowRight"), Some((0x7C, 29)));
        assert_eq!(super::map_key("ArrowDown", "ArrowDown"), Some((0x7D, 31)));
        assert_eq!(super::map_key("ArrowUp", "ArrowUp"), Some((0x7E, 30)));
    }

    #[test]
    fn numlock_off_keypad_arrows_keep_physical_keypad_identity() {
        assert_eq!(super::map_key("ArrowLeft", "Numpad4"), Some((0x56, b'4')));
        assert_eq!(super::map_key("ArrowRight", "Numpad6"), Some((0x58, b'6')));
        assert_eq!(super::map_key("ArrowDown", "Numpad2"), Some((0x54, b'2')));
        assert_eq!(super::map_key("ArrowUp", "Numpad8"), Some((0x5B, b'8')));
    }

    #[test]
    fn numpad_digit_keys_keep_digit_char_codes() {
        assert_eq!(super::map_key("8", "Numpad8"), Some((0x5B, b'8')));
        assert_eq!(super::map_key("4", "Numpad4"), Some((0x56, b'4')));
        assert_eq!(super::map_key("6", "Numpad6"), Some((0x58, b'6')));
    }

    #[test]
    fn shift_keys_update_mac_modifier_state() {
        assert_eq!(super::map_key("Shift", "ShiftLeft"), Some((0x38, 0)));
        assert_eq!(super::map_key("Shift", "ShiftRight"), Some((0x3C, 0)));
    }

    #[test]
    fn perf_toggle_key_is_host_only() {
        assert_eq!(super::map_key("F3", "F3"), None);
    }

    #[test]
    fn mobile_control_keys_map_to_mac_key_codes() {
        assert_eq!(super::mobile_key_code("Space"), Some((0x31, 0x20)));
        assert_eq!(super::mobile_key_code("Enter"), Some((0x24, 0x0D)));
        assert_eq!(super::mobile_key_code("Tab"), Some((0x30, 0x09)));
        assert_eq!(super::mobile_key_code("ArrowUp"), Some((0x7E, 30)));
        assert_eq!(super::mobile_key_code("["), Some((0x21, b'[')));
        assert_eq!(super::mobile_key_code("]"), Some((0x1E, b']')));
        assert_eq!(super::mobile_key_code("\\"), Some((0x2A, b'\\')));
        assert_eq!(super::mobile_key_code("="), Some((0x18, b'=')));
        assert_eq!(super::mobile_key_code("-"), Some((0x1B, b'-')));
        assert_eq!(super::mobile_key_code("g"), Some((0x05, b'g')));
        assert_eq!(super::mobile_key_code("3"), Some((0x14, b'3')));
        assert_eq!(super::mobile_key_code("j"), Some((0x26, b'j')));
        assert_eq!(super::mobile_key_code("l"), Some((0x25, b'l')));
        assert_eq!(super::mobile_key_code("m"), Some((0x2E, b'm')));
        assert_eq!(super::mobile_key_code("None"), None);
        assert_eq!(super::mobile_key_code("Unsupported"), None);
    }

    #[test]
    fn plugin_selection_storage_key_is_scoped_to_game() {
        assert_eq!(
            super::plugin_selection_storage_key("example-title"),
            "systemless.plugin-selection.example-title"
        );
    }

    #[test]
    fn boot_progress_status_tracks_mount_and_runtime_phases() {
        assert_eq!(
            super::boot_progress_status(crate::emulator::BootProgress::MountingArchive {
                loaded_bytes: 512,
                total_bytes: 1024,
            }),
            "Mounting game data (50%)\u{2026}"
        );
        assert_eq!(
            super::boot_progress_status(crate::emulator::BootProgress::LoadingExecutable),
            "Loading executable\u{2026}"
        );
        assert_eq!(
            super::boot_progress_status(crate::emulator::BootProgress::StartingRuntime),
            "Starting Mac runtime\u{2026}"
        );
        assert_eq!(
            super::boot_progress_status(crate::emulator::BootProgress::PreparingAudio),
            "Preparing audio\u{2026}"
        );
    }

    #[test]
    fn fetch_copy_chunks_stay_bounded_for_startup_smoothness() {
        assert_eq!(
            super::FETCH_COPY_YIELD_BYTES,
            256 * 1024,
            "EV/EVO web startup depends on fetch copy slices staying comfortably \
             below long-task scale while larger frame-yield cadence controls total startup latency"
        );
        assert_eq!(
            super::FETCH_COPY_FRAME_YIELD_BYTES,
            1024 * 1024,
            "prefetched archive copies should still force periodic browser frames \
             during large EV/EVO payloads"
        );
        assert_eq!(
            super::FETCH_COPY_FRAME_YIELD_BYTES % super::FETCH_COPY_YIELD_BYTES,
            0
        );
    }

    #[test]
    fn canvas_pointer_events_keep_single_primary_touch() {
        assert!(super::primary_pointer("mouse", false));
        assert!(super::primary_pointer("touch", true));
        assert!(!super::primary_pointer("touch", false));
    }

    #[test]
    fn mobile_controls_accept_secondary_touch_pointers() {
        assert!(super::mobile_control_pointer("touch", true));
        assert!(super::mobile_control_pointer("touch", false));
        assert!(super::mobile_control_pointer("pen", false));
        assert!(super::mobile_control_pointer("", false));
        assert!(super::mobile_control_pointer("mouse", true));
        assert!(!super::mobile_control_pointer("mouse", false));
    }

    #[test]
    fn render_loop_skips_canvas_upload_when_frame_has_no_visual_work() {
        let idle = super::FrameRunResult {
            running: true,
            visual_work: false,
        };
        let visual = super::FrameRunResult {
            running: true,
            visual_work: true,
        };

        assert!(
            super::should_paint_frame(idle, false, false, false),
            "the first frame must paint even if no guest CPU ran yet"
        );
        assert!(
            !super::should_paint_frame(idle, false, true, false),
            "idle audio-only rAF callbacks should not upload an identical canvas frame"
        );
        assert!(
            super::should_paint_frame(idle, true, true, false),
            "screen-mode changes must repaint even if the frame did no visual work"
        );
        assert!(
            super::should_paint_frame(visual, false, true, false),
            "guest CPU work may have changed pixels and must be presented"
        );
        assert!(
            super::should_paint_frame(idle, false, true, true),
            "debug overlay toggles and live stats must force a canvas repaint"
        );
    }
}

fn start_render_loop(
    canvas: HtmlCanvasElement,
    machine: Rc<RefCell<Machine>>,
    alive: Arc<AtomicBool>,
    debug_visible: RwSignal<bool>,
    on_first_paint: Option<Box<dyn FnOnce()>>,
    save_files: RwSignal<Vec<DownloadableSaveFile>>,
    status: RwSignal<String>,
    render_slot: Rc<RefCell<Option<BrowserRenderLoop>>>,
) {
    let (w, h) = machine.borrow().screen_size();
    canvas.set_width(w.max(1));
    canvas.set_height(h.max(1));
    sync_canvas_aspect(&canvas, w, h);
    let mut save_files_version = {
        let machine = machine.borrow();
        save_files.set(machine.save_files().to_vec());
        machine.save_files_version()
    };

    let cb_cell: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let cb_cell_self = cb_cell.clone();
    let request_id = Rc::new(Cell::new(None));
    let request_for_callback = request_id.clone();
    *render_slot.borrow_mut() = Some(BrowserRenderLoop {
        callback: cb_cell.clone(),
        request: request_id.clone(),
    });
    let alive_cb = alive.clone();
    let Some(mut frame) = CanvasFrame::new(&canvas, w, h) else {
        return;
    };
    let _ = canvas.set_attribute("data-render-backend", frame.backend_name());
    let mut canvas_w = w;
    let mut canvas_h = h;
    let mut painted_once = false;
    let mut perf = PerfSampler::new();
    let mut previous_debug_enabled = false;
    let on_first_paint = Rc::new(RefCell::new(on_first_paint));

    *cb_cell.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        request_for_callback.set(None);
        if !alive_cb.load(Ordering::Relaxed) {
            // Component unmounted. Drop the stored closure so the Rc<RefCell<Machine>>
            // capture chain breaks and the Machine can drop (closing audio, freeing RAM).
            *cb_cell_self.borrow_mut() = None;
            return;
        }
        let debug_enabled = debug_visible.get_untracked();
        let debug_visibility_changed = debug_enabled != previous_debug_enabled;
        previous_debug_enabled = debug_enabled;
        let frame_trace = frame_trace_array();
        let trace_enabled = frame_trace.is_some();
        let timing_enabled = debug_enabled || trace_enabled;
        let frame_start_ms = if timing_enabled { perf_now_ms() } else { 0.0 };
        let mut timings = FrameStageTimings::default();
        let mut m = machine.borrow_mut();
        let run_start_ms = if timing_enabled { perf_now_ms() } else { 0.0 };
        let output_scale = canvas_output_scale(&canvas, m.screen_size());
        m.set_output_scale(output_scale);
        let frame_result = m.run_frame();
        let save_error = m.take_save_error();
        let current_save_files_version = m.save_files_version();
        let updated_save_files = if current_save_files_version != save_files_version {
            save_files_version = current_save_files_version;
            Some(m.save_files().to_vec())
        } else {
            None
        };
        if timing_enabled {
            timings.run_ms = perf_now_ms() - run_start_ms;
        }
        let (w, h) = m.screen_size();

        let size_changed = canvas_w != w || canvas_h != h;
        if canvas_w != w {
            canvas_w = w;
        }
        if canvas_h != h {
            canvas_h = h;
        }
        if size_changed {
            sync_canvas_aspect(&canvas, w, h);
        }

        let force_debug_paint = debug_enabled || debug_visibility_changed;
        let painted_frame =
            should_paint_frame(frame_result, size_changed, painted_once, force_debug_paint);
        if painted_frame {
            let render_start_ms = if timing_enabled { perf_now_ms() } else { 0.0 };
            let ((pixel_w, pixel_h), rgba) =
                m.render_rgba(debug_enabled.then(|| perf.frame_stats()));
            if canvas.width() != pixel_w {
                canvas.set_width(pixel_w);
            }
            if canvas.height() != pixel_h {
                canvas.set_height(pixel_h);
            }
            let _ = canvas.set_attribute("data-output-scale", &(pixel_w / w.max(1)).to_string());
            if timing_enabled {
                timings.render_ms = perf_now_ms() - render_start_ms;
            }
            let paint_start_ms = if timing_enabled { perf_now_ms() } else { 0.0 };
            frame.paint(pixel_w, pixel_h, rgba);
            if timing_enabled {
                timings.paint_ms = perf_now_ms() - paint_start_ms;
            }
            if !painted_once {
                if let Some(callback) = on_first_paint.borrow_mut().take() {
                    callback();
                }
            }
            painted_once = true;
        }
        let counters = if timing_enabled {
            let counters = m.perf_counters();
            let frame_ms = perf_now_ms() - frame_start_ms;
            timings.total_ms = frame_ms;
            if debug_enabled {
                perf.sample(counters, frame_start_ms, frame_ms);
            }
            Some(counters)
        } else {
            None
        };
        drop(m);
        if let Some(updated_save_files) = updated_save_files {
            save_files.set(updated_save_files);
        }
        if timing_enabled {
            if let Some(counters) = counters {
                if let Some(trace) = frame_trace.as_ref() {
                    record_frame_trace(
                        trace,
                        frame_start_ms,
                        timings,
                        counters,
                        frame_result,
                        size_changed,
                        painted_frame,
                    );
                }
            }
        }

        if let Some(error) = save_error {
            set_status(status, error);
        }
        if frame_result.running {
            schedule_runtime_raf(&cb_cell_self, &request_for_callback);
        } else {
            // Emulator halted — break the capture chain so we don't leak.
            *cb_cell_self.borrow_mut() = None;
        }
    }) as Box<dyn FnMut()>));

    schedule_runtime_raf(&cb_cell, &request_id);
}

struct WorkerFrameState {
    save_error: Option<String>,
    output_scale: u32,
    frame: Option<(u32, u32, Vec<u8>)>,
    // Keep transferred RGBA buffers as JS views so WebGL can upload them
    // without copying through Wasm memory first.
    js_frame: Option<(u32, u32, Uint8Array)>,
    gpu_frame: Option<JsValue>,
    owned_frame: Option<JsValue>,
    running: bool,
    requests: WorkerFrameRequests<Object>,
}

// Keep one latest display request while the worker is busy. A completion can
// service a missed refresh immediately instead of idling until the next rAF.
// Demand still comes from rAF, so a fast worker never runs an unbounded loop.
struct WorkerFrameRequests<T> {
    in_flight: bool,
    pending: Option<T>,
}

impl<T> Default for WorkerFrameRequests<T> {
    fn default() -> Self {
        Self {
            in_flight: false,
            pending: None,
        }
    }
}

impl<T> WorkerFrameRequests<T> {
    fn request(&mut self, request: T) -> Option<T> {
        if self.in_flight {
            self.pending = Some(request);
            None
        } else {
            self.in_flight = true;
            Some(request)
        }
    }

    fn complete(&mut self, running: bool) -> Option<T> {
        self.in_flight = false;
        if !running {
            self.pending = None;
            return None;
        }
        let next = self.pending.take()?;
        self.in_flight = true;
        Some(next)
    }
}

enum WorkerVisualFrame {
    Gpu(JsValue),
    Owned(JsValue),
    Software(u32, u32, Vec<u8>),
    SoftwareJs(u32, u32, Uint8Array),
}

fn replace_worker_visual_frame(state: &mut WorkerFrameState, frame: WorkerVisualFrame) {
    // Catch-up completions can arrive before the next paint. Keep only the
    // newest result, including when debug mode changes the rendering backend.
    state.owned_frame = None;
    match frame {
        WorkerVisualFrame::Owned(frame) => {
            state.frame = None;
            state.js_frame = None;
            state.gpu_frame = None;
            state.owned_frame = Some(frame);
        }
        WorkerVisualFrame::Gpu(frame) => {
            state.frame = None;
            state.js_frame = None;
            state.gpu_frame = Some(frame);
        }
        WorkerVisualFrame::Software(width, height, pixels) => {
            state.gpu_frame = None;
            state.js_frame = None;
            state.frame = Some((width, height, pixels));
        }
        WorkerVisualFrame::SoftwareJs(width, height, pixels) => {
            state.gpu_frame = None;
            state.frame = None;
            state.js_frame = Some((width, height, pixels));
        }
    }
}

fn take_worker_visual_frame(state: &mut WorkerFrameState) -> Option<WorkerVisualFrame> {
    if let Some(frame) = state.owned_frame.take() {
        Some(WorkerVisualFrame::Owned(frame))
    } else if let Some(frame) = state.gpu_frame.take() {
        Some(WorkerVisualFrame::Gpu(frame))
    } else if let Some((width, height, pixels)) = state.js_frame.take() {
        Some(WorkerVisualFrame::SoftwareJs(width, height, pixels))
    } else {
        state
            .frame
            .take()
            .map(|(width, height, pixels)| WorkerVisualFrame::Software(width, height, pixels))
    }
}

thread_local! {
    static WORKER_GENERATION: Cell<u32> = const { Cell::new(0) };
}

type PendingWorker = StoredValue<Rc<RefCell<Option<Worker>>>, LocalStorage>;

struct BrowserRenderLoop {
    callback: Rc<RefCell<Option<Closure<dyn FnMut()>>>>,
    request: Rc<Cell<Option<i32>>>,
}

impl Drop for BrowserRenderLoop {
    fn drop(&mut self) {
        if let (Some(window), Some(request)) = (web_sys::window(), self.request.take()) {
            let _ = window.cancel_animation_frame(request);
        }
        self.callback.borrow_mut().take();
    }
}

fn schedule_runtime_raf(
    callback: &Rc<RefCell<Option<Closure<dyn FnMut()>>>>,
    request: &Cell<Option<i32>>,
) {
    if let (Some(window), Some(callback)) = (web_sys::window(), callback.borrow().as_ref()) {
        request.set(
            window
                .request_animation_frame(callback.as_ref().unchecked_ref())
                .ok(),
        );
    }
}

type InputListeners = Rc<RefCell<Vec<InputListener>>>;

#[derive(Clone)]
struct InputListener {
    target: web_sys::EventTarget,
    event_name: String,
    callback: js_sys::Function,
    _owner: Rc<dyn std::any::Any>,
}

impl InputListener {
    fn retain<T: ?Sized + 'static>(
        target: &web_sys::EventTarget,
        event_name: &str,
        callback: Closure<T>,
    ) -> Self {
        Self {
            target: target.clone(),
            event_name: event_name.into(),
            callback: callback
                .as_ref()
                .unchecked_ref::<js_sys::Function>()
                .clone(),
            _owner: Rc::new(callback),
        }
    }
}

impl Drop for InputListener {
    fn drop(&mut self) {
        let _ = self
            .target
            .remove_event_listener_with_callback(&self.event_name, &self.callback);
    }
}

fn retain_pointer_cancel(
    listeners: &InputListeners,
    target: &web_sys::EventTarget,
    callback: Closure<dyn FnMut(PointerEvent)>,
) {
    let mut lost_capture = InputListener::retain(target, "pointercancel", callback);
    listeners.borrow_mut().push(lost_capture.clone());
    lost_capture.event_name = "lostpointercapture".into();
    if target
        .add_event_listener_with_callback(&lost_capture.event_name, &lost_capture.callback)
        .is_ok()
    {
        listeners.borrow_mut().push(lost_capture);
    }
}

fn halt_worker(
    worker: &Worker,
    state: &RefCell<WorkerFrameState>,
    audio: &RefCell<Option<WebAudioBackend>>,
    failed: &Cell<bool>,
    listeners: &RefCell<Vec<InputListener>>,
    render_loop: &RefCell<Option<BrowserRenderLoop>>,
) {
    failed.set(true);
    listeners.borrow_mut().clear();
    render_loop.borrow_mut().take();
    worker.set_onmessage(None);
    worker.set_onerror(None);
    worker.set_onmessageerror(None);
    cancel_systemless_worker(worker);
    audio.borrow_mut().take();
    let mut state = state.borrow_mut();
    state.running = false;
    state.requests = WorkerFrameRequests::default();
    state.frame = None;
    state.js_frame = None;
    state.owned_frame = None;
    state.gpu_frame = None;
}

struct WorkerRuntime {
    worker: Worker,
    game_id: &'static str,
    generation: u32,
    stopped: Cell<bool>,
    failed: Rc<Cell<bool>>,
    status: RwSignal<String>,
    next_save_request: Cell<u32>,
    pending_save_requests: Rc<RefCell<std::collections::BTreeSet<u32>>>,
    listeners: InputListeners,
    render_loop: Rc<RefCell<Option<BrowserRenderLoop>>>,
    presenter: RefCell<Option<Rc<RefCell<CanvasFrame>>>>,
    state: Rc<RefCell<WorkerFrameState>>,
    audio: Rc<RefCell<Option<WebAudioBackend>>>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(Event)>,
    _on_message_error: Closure<dyn FnMut(Event)>,
}

impl Drop for WorkerRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

impl WorkerRuntime {
    fn stop(&self) {
        if self.stopped.replace(true) {
            return;
        }
        let failed = self.failed.replace(true);
        self.render_loop.borrow_mut().take();
        self.presenter.borrow_mut().take();
        self.worker.set_onmessage(None);
        self.worker.set_onerror(None);
        self.worker.set_onmessageerror(None);
        if failed {
            cancel_systemless_worker(&self.worker);
        } else {
            let shutdown = shutdown_systemless_worker(&self.worker, self.generation, self.game_id);
            spawn_local(async move {
                if let Err(error) = JsFuture::from(shutdown).await {
                    crate::app::report_runtime_notice(format!(
                        "Could not finish saving the previous game: {}",
                        js_value_string(error)
                    ));
                }
            });
        }
        self.listeners.borrow_mut().clear();
        self.pending_save_requests.borrow_mut().clear();
        self.audio.borrow_mut().take();
        let mut state = self.state.borrow_mut();
        state.running = false;
        state.requests = WorkerFrameRequests::default();
        state.frame = None;
        state.js_frame = None;
        state.owned_frame = None;
        state.gpu_frame = None;
    }

    fn post(&self, message: &Object) -> Result<(), String> {
        if self.failed.get() {
            return Err("Runtime worker has stopped".into());
        }
        set_js_property(
            message,
            "generation",
            &JsValue::from_f64(self.generation as f64),
        );
        post_systemless_worker_command(&self.worker, message, &Array::new()).map_err(|error| {
            self.failed.set(true);
            let error = js_value_string(error);
            let _ = self
                .status
                .try_set(format!("Runtime worker stopped: {error}"));
            error
        })
    }

    fn begin_save_request(&self, message: &Object) -> Result<u32, String> {
        if self.failed.get() {
            return Err("Runtime worker has stopped".into());
        }
        let mut pending = self.pending_save_requests.borrow_mut();
        if pending.len() >= 32 {
            return Err("Save operations are still pending; please wait".into());
        }
        let id = self
            .next_save_request
            .get()
            .checked_add(1)
            .ok_or("Save request sequence exhausted")?;
        self.next_save_request.set(id);
        pending.insert(id);
        set_js_property(message, "requestId", &JsValue::from_f64(id as f64));
        Ok(id)
    }

    fn import_save(&self, bytes: &[u8]) -> Result<(), String> {
        let bytes = Uint8Array::from(bytes);
        let message = Object::new();
        set_js_property(&message, "type", &JsValue::from_str("importSave"));
        set_js_property(&message, "bytes", bytes.buffer().as_ref());
        let transfer = Array::new();
        transfer.push(bytes.buffer().as_ref());
        set_js_property(
            &message,
            "generation",
            &JsValue::from_f64(self.generation as f64),
        );
        let request = self.begin_save_request(&message)?;
        let result = post_systemless_worker_command(&self.worker, &message, &transfer)
            .map_err(js_value_string);
        if result.is_err() {
            self.pending_save_requests.borrow_mut().remove(&request);
        }
        result
    }

    fn delete_save(&self, path: &str) -> Result<(), String> {
        let message = Object::new();
        set_js_property(&message, "type", &JsValue::from_str("deleteSave"));
        set_js_property(&message, "path", &JsValue::from_str(path));
        let request = self.begin_save_request(&message)?;
        let result = self.post(&message);
        if result.is_err() {
            self.pending_save_requests.borrow_mut().remove(&request);
        }
        result
    }
}

async fn boot_catalogue_worker(
    game_bytes: &[u8],
    plugin_files: &[PluginFile],
    game: &Game,
    architecture: GameArchitecture,
    save_files: RwSignal<Vec<DownloadableSaveFile>>,
    status: RwSignal<String>,
    pending_worker: PendingWorker,
    alive: Arc<AtomicBool>,
    audio_bootstrap: Rc<RefCell<Option<AudioBootstrap>>>,
    input_listeners: InputListeners,
) -> Result<Rc<WorkerRuntime>, String> {
    let assets = systemless_runtime_assets();
    if assets.length() != 2 {
        return Err("runtime assets were not discoverable".to_string());
    }
    let module_url = assets
        .get(0)
        .as_string()
        .ok_or("runtime module is missing")?;
    let wasm_url = assets.get(1).as_string().ok_or("runtime wasm is missing")?;
    let runtime_id = module_url.rsplit('/').next().unwrap_or("current");
    let worker_url = format!("/emulator-worker.js?runtime={runtime_id}");
    let worker = Worker::new(&worker_url).map_err(js_value_string)?;
    pending_worker.try_with_value(|slot| *slot.borrow_mut() = Some(worker.clone()));
    let generation = WORKER_GENERATION.with(|counter| {
        let next = counter
            .get()
            .checked_add(1)
            .expect("runtime generation exhausted");
        counter.set(next);
        next
    });
    // Transfer a JS copy, retaining the Rust archive for startup fallback.
    let bytes = Uint8Array::from(game_bytes);
    let message = Object::new();
    set_js_property(&message, "type", &JsValue::from_str("boot"));
    set_js_property(
        &message,
        "generation",
        &JsValue::from_f64(generation as f64),
    );
    set_js_property(&message, "protocolVersion", &JsValue::from_f64(5.0));
    set_js_property(&message, "moduleUrl", &JsValue::from_str(&module_url));
    set_js_property(&message, "wasmUrl", &JsValue::from_str(&wasm_url));
    set_js_property(&message, "gameBytes", bytes.buffer().as_ref());
    let config = serde_json::json!({
        "id": game.id, "architecture": architecture.key(),
        "launch_modifiers": game.settings.launch_modifiers,
        "show_menu_bar": game.settings.show_menu_bar,
        "screen_depth": game.settings.screen_depth,
        "application_partition_size": game.settings.application_partition_size,
        "remove_paths": game.settings.remove_paths,
        "file_mappings": game.settings.file_mappings,
        "runtime_pacing": game.settings.runtime_pacing,
        "arrows_as_numpad": game.settings.arrows_as_numpad,
        "plugins": plugin_files.iter().map(crate::worker_runtime::PluginMetadata::from).collect::<Vec<_>>(),
    });
    set_js_property(&message, "config", &JsValue::from_str(&config.to_string()));
    let transfer = Array::new();
    transfer.push(bytes.buffer().as_ref());
    let plugin_forks = Array::new();
    for plugin in plugin_files {
        for fork in [&plugin.file.data_fork, &plugin.file.resource_fork] {
            let bytes = Uint8Array::from(fork.as_slice());
            plugin_forks.push(bytes.buffer().as_ref());
            transfer.push(bytes.buffer().as_ref());
        }
    }
    set_js_property(&message, "pluginForks", plugin_forks.as_ref());
    let on_progress = Closure::wrap(Box::new(move |value: JsValue| {
        if let Some(progress) = value
            .as_string()
            .and_then(|value| serde_json::from_str(&value).ok())
        {
            let _ = status.try_set(boot_progress_status(progress));
        }
    }) as Box<dyn FnMut(JsValue)>);
    let ready = match JsFuture::from(boot_systemless_worker(
        &worker,
        &message,
        &transfer,
        15_000,
        on_progress.as_ref().unchecked_ref(),
    ))
    .await
    {
        Ok(ready) => ready,
        Err(error) => {
            cancel_systemless_worker(&worker);
            pending_worker.try_with_value(|slot| slot.borrow_mut().take());
            return Err(js_value_string(error));
        }
    };
    if !alive.load(Ordering::Relaxed) {
        cancel_systemless_worker(&worker);
        return Err("Runtime startup cancelled".into());
    }
    if let Some(files) = downloadable_save_files_from_js(&ready) {
        save_files.set(files);
    }

    let state = Rc::new(RefCell::new(WorkerFrameState {
        save_error: None,
        output_scale: 1,
        frame: None,
        js_frame: None,
        gpu_frame: None,
        owned_frame: None,
        running: true,
        requests: WorkerFrameRequests::default(),
    }));
    let audio = Rc::new(RefCell::new(
        WebAudioBackend::from_bootstrap(&audio_bootstrap).await,
    ));
    if !alive.load(Ordering::Relaxed) {
        cancel_systemless_worker(&worker);
        return Err("Runtime startup cancelled".into());
    }
    pending_worker.try_with_value(|slot| slot.borrow_mut().take());
    let failed = Rc::new(Cell::new(false));
    let failed_for_message = failed.clone();
    let state_for_message = state.clone();
    let audio_for_message = audio.clone();
    let worker_for_message = worker.clone();
    let pending_save_requests = Rc::new(RefCell::new(std::collections::BTreeSet::new()));
    let pending_saves_for_message = pending_save_requests.clone();
    let save_version = Cell::new(
        js_string_property(&ready, "saveFilesVersion")
            .and_then(|version| version.parse::<u64>().ok())
            .unwrap_or(0),
    );
    let render_loop = Rc::new(RefCell::new(None));
    let render_for_message = render_loop.clone();
    let listeners_for_message = input_listeners.clone();
    let last_sequence = Cell::new(0.0);
    let on_message = Closure::wrap(Box::new(move |event: MessageEvent| {
        let data = event.data();
        if js_number_property(&data, "generation") != Some(generation as f64) {
            return;
        }
        if js_string_property(&data, "type").as_deref() == Some("commandAck") {
            if let Some(sequence) = js_number_property(&data, "commandSequence") {
                if let Err(error) =
                    acknowledge_systemless_worker_command(&worker_for_message, generation, sequence)
                {
                    set_status(
                        status,
                        format!("Runtime worker stopped: {}", js_value_string(error)),
                    );
                    halt_worker(
                        &worker_for_message,
                        &state_for_message,
                        &audio_for_message,
                        &failed_for_message,
                        &listeners_for_message,
                        &render_for_message,
                    );
                }
            }
            return;
        }
        if let Some(request) = js_number_property(&data, "requestId") {
            pending_saves_for_message
                .borrow_mut()
                .remove(&(request as u32));
        }
        if js_string_property(&data, "type").as_deref() == Some("error") {
            let message = js_string_property(&data, "message")
                .unwrap_or_else(|| "Unknown worker error".into());
            let _ = status.try_set(format!("Runtime worker: {message}"));
            if js_bool_property(&data, "fatal").unwrap_or(true) {
                halt_worker(
                    &worker_for_message,
                    &state_for_message,
                    &audio_for_message,
                    &failed_for_message,
                    &listeners_for_message,
                    &render_for_message,
                );
            }
            return;
        }
        if js_string_property(&data, "type").as_deref() == Some("saveFiles") {
            if let Some(version) = js_string_property(&data, "saveFilesVersion")
                .and_then(|version| version.parse::<u64>().ok())
                .filter(|version| *version >= save_version.get())
            {
                if let Some(files) = downloadable_save_files_from_js(&data) {
                    save_version.set(version);
                    let _ = save_files.try_set(files);
                }
            }
            return;
        }
        if js_string_property(&data, "type").as_deref() != Some("frame") {
            return;
        }
        let Some(sequence) = js_number_property(&data, "sequence") else {
            return;
        };
        if sequence <= last_sequence.get() {
            return;
        }
        last_sequence.set(sequence);
        let mut state = state_for_message.borrow_mut();
        if let Some(error) = js_string_property(&data, "saveError") {
            state.save_error = Some(error);
        }
        state.running = js_bool_property(&data, "running").unwrap_or(false);
        if let Ok(audio_bytes) = Reflect::get(&data, &JsValue::from_str("audio")) {
            if !audio_bytes.is_undefined() {
                let samples = Uint8Array::new(&audio_bytes).to_vec();
                if !samples.is_empty() {
                    if let Some(audio) = audio_for_message.borrow_mut().as_mut() {
                        audio.queue_samples(&samples);
                    }
                }
            }
        }
        if let Ok(frame) = Reflect::get(&data, &JsValue::from_str("frame")) {
            if !frame.is_undefined() {
                let width = js_number_property(&data, "width").unwrap_or(1.0) as u32;
                let height = js_number_property(&data, "height").unwrap_or(1.0) as u32;
                state.output_scale = js_number_property(&data, "outputScale").unwrap_or(1.0) as u32;
                replace_worker_visual_frame(
                    &mut state,
                    WorkerVisualFrame::SoftwareJs(width, height, Uint8Array::new(&frame)),
                );
            }
        }
        if let Ok(frame) = Reflect::get(&data, &JsValue::from_str("gpuFrame")) {
            if !frame.is_undefined() {
                replace_worker_visual_frame(&mut state, WorkerVisualFrame::Gpu(frame));
            }
        }
        if let Ok(frame) = Reflect::get(&data, &JsValue::from_str("indexedFrame")) {
            if !frame.is_undefined() {
                state.output_scale = 1;
                replace_worker_visual_frame(&mut state, WorkerVisualFrame::Owned(frame));
            }
        }
        if let Ok(frame) = Reflect::get(&data, &JsValue::from_str("compactFrame")) {
            if !frame.is_undefined() {
                state.output_scale = js_number_property(&data, "outputScale").unwrap_or(1.0) as u32;
                replace_worker_visual_frame(&mut state, WorkerVisualFrame::Owned(frame));
            }
        }
        let running = state.running;
        let next = state.requests.complete(running);
        drop(state);
        if let Some(message) = next {
            // Audio was just queued above; use its current depth rather than
            // the snapshot taken at the missed display refresh.
            set_js_property(
                &message,
                "queuedAudioSamples",
                &JsValue::from_f64(worker_audio_queue_samples(&audio_for_message) as f64),
            );
            set_js_property(
                &message,
                "generation",
                &JsValue::from_f64(generation as f64),
            );
            if let Err(error) =
                post_systemless_worker_command(&worker_for_message, &message, &Array::new())
            {
                let _ = status.try_set(format!(
                    "Runtime worker stopped: {}",
                    js_value_string(error)
                ));
                halt_worker(
                    &worker_for_message,
                    &state_for_message,
                    &audio_for_message,
                    &failed_for_message,
                    &listeners_for_message,
                    &render_for_message,
                );
            }
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    let make_error_handler = |fallback: &'static str| {
        let worker = worker.clone();
        let state = state.clone();
        let audio = audio.clone();
        let failed = failed.clone();
        let listeners = input_listeners.clone();
        let render = render_loop.clone();
        Closure::wrap(Box::new(move |event: Event| {
            let message =
                js_string_property(event.as_ref(), "message").unwrap_or_else(|| fallback.into());
            let _ = status.try_set(format!("Runtime worker stopped: {message}"));
            halt_worker(&worker, &state, &audio, &failed, &listeners, &render);
        }) as Box<dyn FnMut(Event)>)
    };
    let on_error = make_error_handler("Worker crashed");
    let on_message_error = make_error_handler("Unreadable worker reply");
    worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    worker.set_onmessageerror(Some(on_message_error.as_ref().unchecked_ref()));
    Ok(Rc::new(WorkerRuntime {
        worker,
        game_id: game.id,
        generation,
        stopped: Cell::new(false),
        failed,
        status,
        next_save_request: Cell::new(0),
        pending_save_requests,
        listeners: input_listeners,
        render_loop,
        presenter: RefCell::new(None),
        state,
        audio,
        _on_message: on_message,
        _on_error: on_error,
        _on_message_error: on_message_error,
    }))
}

fn start_worker_render_loop(
    canvas: HtmlCanvasElement,
    runtime: Rc<WorkerRuntime>,
    alive: Arc<AtomicBool>,
    debug_visible: RwSignal<bool>,
    on_first_paint: Box<dyn FnOnce()>,
) {
    let Some(renderer) = CanvasFrame::new_worker(&canvas, 640, 480, runtime.generation) else {
        set_status(
            runtime.status,
            "Unable to initialize the game display".into(),
        );
        runtime.stop();
        return;
    };
    let _ = canvas.set_attribute("data-render-backend", renderer.backend_name());
    let gpu_enabled = renderer.supports_q3_gpu();
    if gpu_enabled {
        let message = Object::new();
        set_js_property(&message, "type", &JsValue::from_str("enableGpu"));
        set_js_property(&message, "enabled", &JsValue::TRUE);
        let _ = runtime.post(&message);
    }
    // The lease outlives the rAF loop so guest exit preserves the final image.
    let presenter = Rc::new(RefCell::new(renderer));
    *runtime.presenter.borrow_mut() = Some(presenter.clone());
    let callback_cell: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let callback_self = callback_cell.clone();
    let request_id = Rc::new(Cell::new(None));
    let request_for_callback = request_id.clone();
    *runtime.render_loop.borrow_mut() = Some(BrowserRenderLoop {
        callback: callback_cell.clone(),
        request: request_id.clone(),
    });
    let first_paint = Rc::new(RefCell::new(Some(on_first_paint)));
    *callback_cell.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        request_for_callback.set(None);
        if !alive.load(Ordering::Relaxed) || runtime.failed.get() {
            runtime.stop();
            *callback_self.borrow_mut() = None;
            return;
        }
        let mut renderer = presenter.borrow_mut();
        if let Err(error) = renderer.poll_recovery() {
            set_status(runtime.status, error);
        }
        let _ = canvas.set_attribute("data-render-backend", renderer.backend_name());
        let mut painted = false;
        let visual_frame = {
            let mut state = runtime.state.borrow_mut();
            take_worker_visual_frame(&mut state)
        };
        match visual_frame {
            Some(WorkerVisualFrame::Owned(frame)) => {
                let width = js_number_property(&frame, "width").unwrap_or(1.0) as u32;
                let height = js_number_property(&frame, "height").unwrap_or(1.0) as u32;
                let _ = canvas.set_attribute(
                    "data-output-scale",
                    &runtime.state.borrow().output_scale.to_string(),
                );
                if canvas.width() != width {
                    canvas.set_width(width);
                }
                if canvas.height() != height {
                    canvas.set_height(height);
                }
                sync_canvas_aspect(&canvas, width, height);
                renderer.paint_owned(&frame);
            }
            Some(WorkerVisualFrame::Gpu(frame)) => {
                let _ = canvas.set_attribute("data-output-scale", "1");
                let width = js_number_property(&frame, "width").unwrap_or(1.0) as u32;
                let height = js_number_property(&frame, "height").unwrap_or(1.0) as u32;
                if canvas.width() != width.max(1) {
                    canvas.set_width(width.max(1));
                }
                if canvas.height() != height.max(1) {
                    canvas.set_height(height.max(1));
                }
                if renderer.paint_q3(&frame).is_some() {
                    sync_canvas_aspect(&canvas, width, height);
                    let _ = canvas.set_attribute("data-render-backend", "webgl-qd3d");
                    painted = true;
                }
            }
            Some(WorkerVisualFrame::Software(width, height, pixels)) => {
                let _ = canvas.set_attribute(
                    "data-output-scale",
                    &runtime.state.borrow().output_scale.to_string(),
                );
                if canvas.width() != width.max(1) {
                    canvas.set_width(width.max(1));
                }
                if canvas.height() != height.max(1) {
                    canvas.set_height(height.max(1));
                }
                sync_canvas_aspect(&canvas, width, height);
                renderer.paint(width, height, &pixels);
                painted = !renderer.asynchronous();
            }
            Some(WorkerVisualFrame::SoftwareJs(width, height, pixels)) => {
                let _ = canvas.set_attribute(
                    "data-output-scale",
                    &runtime.state.borrow().output_scale.to_string(),
                );
                if canvas.width() != width.max(1) {
                    canvas.set_width(width.max(1));
                }
                if canvas.height() != height.max(1) {
                    canvas.set_height(height.max(1));
                }
                sync_canvas_aspect(&canvas, width, height);
                renderer.paint_js(width, height, &pixels);
                painted = !renderer.asynchronous();
            }
            None => {}
        }
        if painted || renderer.submitted() {
            if let Some(callback) = first_paint.borrow_mut().take() {
                callback();
            }
        }
        let indexed_render = renderer.supports_packet("indexed8");
        let compact_render = renderer.supports_packet("compact");
        let force_snapshot = renderer.needs_snapshot();
        let presentation_pending = renderer.pending();
        // A stopped guest can still lose its offscreen context. Keep polling
        // presenter status without asking the owner to execute guest work.
        let monitor_presenter = renderer.asynchronous();
        drop(renderer);
        let mut state = runtime.state.borrow_mut();
        if let Some(error) = state.save_error.take() {
            set_status(runtime.status, error);
        }
        if state.running || force_snapshot {
            let queued = worker_audio_queue_samples(&runtime.audio);
            let message = Object::new();
            set_js_property(&message, "type", &JsValue::from_str("frame"));
            set_js_property(&message, "forceRender", &JsValue::from_bool(force_snapshot));
            set_js_property(
                &message,
                "indexedRender",
                &JsValue::from_bool(indexed_render),
            );
            set_js_property(
                &message,
                "compactRender",
                &JsValue::from_bool(compact_render),
            );
            let scale = canvas_backing_scale(&canvas);
            let logical = (canvas.width() / scale, canvas.height() / scale);
            set_js_property(
                &message,
                "outputScale",
                &JsValue::from_f64(canvas_output_scale(&canvas, logical) as f64),
            );
            set_js_property(
                &message,
                "queuedAudioSamples",
                &JsValue::from_f64(queued as f64),
            );
            set_js_property(
                &message,
                "debug",
                &JsValue::from_bool(debug_visible.get_untracked()),
            );
            if let Some(message) = state.requests.request(message) {
                let _ = runtime.post(&message);
            }
        }
        let running = state.running;
        drop(state);
        if running || presentation_pending || force_snapshot || monitor_presenter {
            schedule_runtime_raf(&callback_self, &request_for_callback);
        } else {
            *callback_self.borrow_mut() = None;
        }
    }) as Box<dyn FnMut()>));
    schedule_runtime_raf(&callback_cell, &request_id);
}

fn worker_audio_queue_samples(audio: &RefCell<Option<WebAudioBackend>>) -> i32 {
    audio
        .borrow_mut()
        .as_mut()
        .and_then(WebAudioBackend::queued_source_samples)
        .map_or(-1, |samples| samples.min(i32::MAX as usize) as i32)
}

fn set_js_property(object: &Object, name: &str, value: &JsValue) {
    let _ = Reflect::set(object.as_ref(), &JsValue::from_str(name), value);
}

fn js_string_property(value: &JsValue, name: &str) -> Option<String> {
    Reflect::get(value, &JsValue::from_str(name))
        .ok()?
        .as_string()
}

fn js_number_property(value: &JsValue, name: &str) -> Option<f64> {
    Reflect::get(value, &JsValue::from_str(name)).ok()?.as_f64()
}

fn js_bool_property(value: &JsValue, name: &str) -> Option<bool> {
    Reflect::get(value, &JsValue::from_str(name))
        .ok()?
        .as_bool()
}

fn downloadable_save_files_from_js(value: &JsValue) -> Option<Vec<DownloadableSaveFile>> {
    let files = Array::from(&Reflect::get(value, &JsValue::from_str("files")).ok()?);
    files
        .iter()
        .map(|file| {
            Some(DownloadableSaveFile {
                path: js_string_property(&file, "path")?,
                name: js_string_property(&file, "name")?,
                data_len: js_number_property(&file, "dataLen")? as usize,
                resource_len: js_number_property(&file, "resourceLen")? as usize,
                modified_date: js_number_property(&file, "modifiedDate")? as u32,
                macbinary: Uint8Array::new(
                    &Reflect::get(&file, &JsValue::from_str("macbinary")).ok()?,
                )
                .to_vec(),
            })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default)]
struct FrameStageTimings {
    run_ms: f64,
    render_ms: f64,
    paint_ms: f64,
    total_ms: f64,
}

fn should_paint_frame(
    frame_result: FrameRunResult,
    size_changed: bool,
    painted_once: bool,
    force_paint: bool,
) -> bool {
    !painted_once || size_changed || frame_result.visual_work || force_paint
}

fn frame_trace_array() -> Option<Array> {
    Reflect::get(
        web_sys::window()?.as_ref(),
        &JsValue::from_str("__systemlessFrameTrace"),
    )
    .ok()?
    .dyn_into::<Array>()
    .ok()
}

fn record_frame_trace(
    trace: &Array,
    frame_start_ms: f64,
    timings: FrameStageTimings,
    counters: PerfCounters,
    frame_result: FrameRunResult,
    size_changed: bool,
    painted_frame: bool,
) {
    let entry = Object::new();
    set_trace_number(&entry, "t", frame_start_ms);
    set_trace_number(&entry, "totalMs", timings.total_ms);
    set_trace_number(&entry, "runMs", timings.run_ms);
    set_trace_number(&entry, "renderMs", timings.render_ms);
    set_trace_number(&entry, "paintMs", timings.paint_ms);
    set_trace_number(&entry, "guestTick", counters.guest_tick as f64);
    set_trace_number(
        &entry,
        "totalInstructions",
        counters.total_instructions as f64,
    );
    set_trace_number(&entry, "ticksBehind", counters.ticks_behind as f64);
    set_trace_number(&entry, "lastSteps", counters.last_steps as f64);
    set_trace_number(&entry, "cpuBudgetMs", counters.cpu_budget_ms);
    if let Some(audio_queue_ms) = counters.audio_queue_ms {
        set_trace_number(&entry, "audioQueueMs", audio_queue_ms);
    }
    set_trace_bool(&entry, "visualWork", frame_result.visual_work);
    set_trace_bool(&entry, "sizeChanged", size_changed);
    set_trace_bool(&entry, "painted", painted_frame);
    trace.push(entry.as_ref());
    if trace.length() > 6_000 {
        let _ = trace.shift();
    }
}

fn set_trace_number(entry: &Object, name: &str, value: f64) {
    let _ = Reflect::set(
        entry.as_ref(),
        &JsValue::from_str(name),
        &JsValue::from_f64(value),
    );
}

fn set_trace_bool(entry: &Object, name: &str, value: bool) {
    let _ = Reflect::set(
        entry.as_ref(),
        &JsValue::from_str(name),
        &JsValue::from_bool(value),
    );
}

fn sync_canvas_aspect(canvas: &HtmlCanvasElement, width: u32, height: u32) {
    let width = width.max(1);
    let height = height.max(1);
    let width_s = width.to_string();
    let height_s = height.to_string();
    let ratio = format!("{width} / {height}");

    let canvas_el: &HtmlElement = canvas.unchecked_ref();
    set_game_aspect_vars(&canvas_el.style(), &ratio, &width_s, &height_s);
    if let Some(parent) = canvas
        .parent_element()
        .and_then(|el| el.dyn_into::<HtmlElement>().ok())
    {
        set_game_aspect_vars(&parent.style(), &ratio, &width_s, &height_s);
    }
}

fn set_game_aspect_vars(style: &CssStyleDeclaration, ratio: &str, width: &str, height: &str) {
    let _ = style.set_property("--game-aspect-ratio", ratio);
    let _ = style.set_property("--game-aspect-width", width);
    let _ = style.set_property("--game-aspect-height", height);
}

struct PerfSampler {
    last_update_ms: f64,
    last_frame_start_ms: f64,
    last_guest_tick: u32,
    last_instructions: u64,
    frames: u32,
    frame_ms_sum: f64,
    last_stats: DebugOverlayFrameStats,
}

impl PerfSampler {
    fn new() -> Self {
        let now = perf_now_ms();
        Self {
            last_update_ms: now,
            last_frame_start_ms: now,
            last_guest_tick: 0,
            last_instructions: 0,
            frames: 0,
            frame_ms_sum: 0.0,
            last_stats: DebugOverlayFrameStats::default(),
        }
    }

    fn frame_stats(&self) -> DebugOverlayFrameStats {
        self.last_stats
    }

    fn sample(&mut self, counters: PerfCounters, frame_start_ms: f64, frame_ms: f64) {
        let now = perf_now_ms();
        self.frames = self.frames.saturating_add(1);
        self.frame_ms_sum += frame_ms.max(0.0);

        if self.last_guest_tick == 0 && self.last_instructions == 0 {
            self.last_guest_tick = counters.guest_tick;
            self.last_instructions = counters.total_instructions;
            self.last_frame_start_ms = frame_start_ms;
            self.last_update_ms = now;
            return;
        }

        let elapsed_ms = now - self.last_update_ms;
        if elapsed_ms < 500.0 {
            return;
        }

        let elapsed_s = elapsed_ms / 1000.0;
        let host_fps = self.frames as f64 / elapsed_s;
        let avg_frame_ms = self.frame_ms_sum / self.frames.max(1) as f64;
        let guest_ticks = counters.guest_tick.wrapping_sub(self.last_guest_tick) as f64;
        let guest_ips = counters
            .total_instructions
            .saturating_sub(self.last_instructions) as f64
            / elapsed_s;

        self.last_update_ms = now;
        self.last_frame_start_ms = frame_start_ms;
        self.last_guest_tick = counters.guest_tick;
        self.last_instructions = counters.total_instructions;
        self.frames = 0;
        self.frame_ms_sum = 0.0;

        self.last_stats = DebugOverlayFrameStats {
            host_fps: Some(host_fps),
            frame_ms: Some(avg_frame_ms),
            guest_mips: Some(guest_ips / 1_000_000.0),
            guest_ticks_per_sec: Some(guest_ticks / elapsed_s),
            ticks_behind: Some(counters.ticks_behind),
            last_steps: Some(counters.last_steps),
            cpu_budget_ms: Some(counters.cpu_budget_ms),
            audio_queue_ms: counters.audio_queue_ms,
        };
    }
}

fn attach_input(
    canvas: &HtmlCanvasElement,
    machine: RuntimeHandle,
    alive: Arc<AtomicBool>,
    mappings: &'static [(&'static str, &'static str)],
    listeners: &InputListeners,
) {
    focus_canvas(canvas);

    if pointer_events_supported() {
        attach_pointer_input(canvas, machine.clone(), alive.clone(), listeners);
    } else {
        let suppress_mouse_until_ms = Rc::new(Cell::new(0.0));
        attach_touch_input(
            canvas,
            machine.clone(),
            alive.clone(),
            suppress_mouse_until_ms.clone(),
            listeners,
        );
        attach_mouse_input(
            canvas,
            machine.clone(),
            alive.clone(),
            suppress_mouse_until_ms,
            listeners,
        );
    }

    let pressed = Rc::new(RefCell::new(
        std::collections::BTreeMap::<String, (u8, u8)>::new(),
    ));
    let pressed_kd = pressed.clone();
    let machine_kd = machine.clone();
    let alive_kd = alive.clone();
    let on_key_down = Closure::wrap(Box::new(move |ev: KeyboardEvent| {
        if !alive_kd.load(Ordering::Relaxed) {
            return;
        }
        let m = &machine_kd;
        m.resume_audio();
        if let Some((mac_key, char_code)) = mapped_key(&ev.key(), &ev.code(), mappings) {
            ev.prevent_default();
            pressed_kd
                .borrow_mut()
                .insert(ev.code(), (mac_key, char_code));
            m.key_down(mac_key, char_code);
        }
    }) as Box<dyn FnMut(KeyboardEvent)>);
    let _ =
        canvas.add_event_listener_with_callback("keydown", on_key_down.as_ref().unchecked_ref());
    listeners.borrow_mut().push(InputListener::retain(
        canvas.as_ref(),
        "keydown",
        on_key_down,
    ));

    let pressed_ku = pressed.clone();
    let machine_ku = machine.clone();
    let alive_ku = alive.clone();
    let on_key_up = Closure::wrap(Box::new(move |ev: KeyboardEvent| {
        if !alive_ku.load(Ordering::Relaxed) {
            return;
        }
        pressed_ku.borrow_mut().remove(&ev.code());
        let key = mapped_key(&ev.key(), &ev.code(), mappings);
        if let Some((mac_key, char_code)) = key {
            ev.prevent_default();
            machine_ku.key_up(mac_key, char_code);
        }
    }) as Box<dyn FnMut(KeyboardEvent)>);
    let _ = canvas.add_event_listener_with_callback("keyup", on_key_up.as_ref().unchecked_ref());
    listeners
        .borrow_mut()
        .push(InputListener::retain(canvas.as_ref(), "keyup", on_key_up));
    attach_focus_release(Some(canvas), listeners, move || {
        if alive.load(Ordering::Relaxed) {
            for (_, (mac_key, char_code)) in std::mem::take(&mut *pressed.borrow_mut()) {
                machine.key_up(mac_key, char_code);
            }
        }
    });
}

fn attach_focus_release(
    canvas: Option<&HtmlCanvasElement>,
    listeners: &InputListeners,
    release: impl FnMut() + Clone + 'static,
) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let mut targets = vec![(
        window.clone().unchecked_into::<web_sys::EventTarget>(),
        "blur",
    )];
    if let Some(document) = window.document() {
        targets.push((document.unchecked_into(), "visibilitychange"));
    }
    if let Some(canvas) = canvas {
        targets.push((canvas.clone().unchecked_into(), "blur"));
    }
    for (target, event_name) in targets {
        let mut release = release.clone();
        let callback = Closure::wrap(Box::new(move |event: Event| {
            if event.type_() == "visibilitychange"
                && !web_sys::window()
                    .and_then(|w| w.document())
                    .is_some_and(|d| d.hidden())
            {
                return;
            }
            release();
        }) as Box<dyn FnMut(Event)>);
        if target
            .add_event_listener_with_callback(event_name, callback.as_ref().unchecked_ref())
            .is_ok()
        {
            listeners
                .borrow_mut()
                .push(InputListener::retain(&target, event_name, callback));
        }
    }
}

fn pointer_events_supported() -> bool {
    web_sys::window()
        .and_then(|window| {
            Reflect::has(window.as_ref(), &JsValue::from_str("PointerEvent"))
                .ok()
                .map(Some)
        })
        .flatten()
        .unwrap_or(false)
}

fn attach_pointer_input(
    canvas: &HtmlCanvasElement,
    machine: RuntimeHandle,
    alive: Arc<AtomicBool>,
    listeners: &InputListeners,
) {
    let active_pointer_id: Rc<Cell<Option<i32>>> = Rc::new(Cell::new(None));
    let last_coords: Rc<Cell<Option<(i16, i16)>>> = Rc::new(Cell::new(None));
    let pointer_down_ms: Rc<Cell<f64>> = Rc::new(Cell::new(0.0));
    let touch_like_pointer: Rc<Cell<bool>> = Rc::new(Cell::new(false));

    let focus_machine = machine.clone();
    let focus_alive = alive.clone();
    let focus_canvas_el = canvas.clone();
    let focus_active = active_pointer_id.clone();
    let focus_coords = last_coords.clone();
    attach_focus_release(Some(canvas), listeners, move || {
        if let Some(pointer_id) = focus_active.get() {
            release_pointer_mouse_up_after(
                focus_canvas_el.clone(),
                focus_machine.clone(),
                focus_alive.clone(),
                focus_active.clone(),
                focus_coords.clone(),
                pointer_id,
                focus_coords.get(),
                0.0,
            );
        }
    });

    // Pointer events give touch, pen, and mouse a single path and let us capture
    // the active touch, so a game redraw or finger drift does not lose mouseUp.
    let canvas_el = canvas.clone();
    let machine_pd = machine.clone();
    let alive_pd = alive.clone();
    let active_pd = active_pointer_id.clone();
    let last_pd = last_coords.clone();
    let down_ms_pd = pointer_down_ms.clone();
    let touch_like_pd = touch_like_pointer.clone();
    let on_pointer_down = Closure::wrap(Box::new(move |ev: PointerEvent| {
        if !alive_pd.load(Ordering::Relaxed)
            || !primary_pointer_event(&ev)
            || ev.button() != 0
            || active_pd.get().is_some()
        {
            return;
        }
        focus_canvas(&canvas_el);
        ev.prevent_default();
        let m = &machine_pd;
        m.resume_audio();
        if let Some((v, h)) = pointer_coords(&canvas_el, &ev) {
            active_pd.set(Some(ev.pointer_id()));
            last_pd.set(Some((v, h)));
            down_ms_pd.set(perf_now_ms());
            touch_like_pd.set(ev.pointer_type() != "mouse");
            let _ = canvas_el.set_pointer_capture(ev.pointer_id());
            m.mouse_down(v, h);
        }
    }) as Box<dyn FnMut(PointerEvent)>);
    let _ = canvas
        .add_event_listener_with_callback("pointerdown", on_pointer_down.as_ref().unchecked_ref());
    listeners.borrow_mut().push(InputListener::retain(
        canvas.as_ref(),
        "pointerdown",
        on_pointer_down,
    ));

    let canvas_el = canvas.clone();
    let machine_pm = machine.clone();
    let alive_pm = alive.clone();
    let active_pm = active_pointer_id.clone();
    let last_pm = last_coords.clone();
    let on_pointer_move = Closure::wrap(Box::new(move |ev: PointerEvent| {
        if !alive_pm.load(Ordering::Relaxed) || !primary_pointer_event(&ev) {
            return;
        }
        let active = active_pm.get();
        let pointer_type = ev.pointer_type();
        if pointer_type != "mouse" && active != Some(ev.pointer_id()) {
            return;
        }
        if active == Some(ev.pointer_id()) {
            ev.prevent_default();
        }
        if let Some((v, h)) = pointer_coords(&canvas_el, &ev) {
            if active == Some(ev.pointer_id()) {
                last_pm.set(Some((v, h)));
            }
            machine_pm.mouse_move(v, h);
        }
    }) as Box<dyn FnMut(PointerEvent)>);
    let _ = canvas
        .add_event_listener_with_callback("pointermove", on_pointer_move.as_ref().unchecked_ref());
    listeners.borrow_mut().push(InputListener::retain(
        canvas.as_ref(),
        "pointermove",
        on_pointer_move,
    ));

    let canvas_el = canvas.clone();
    let machine_pu = machine.clone();
    let alive_pu = alive.clone();
    let active_pu = active_pointer_id.clone();
    let last_pu = last_coords.clone();
    let down_ms_pu = pointer_down_ms.clone();
    let touch_like_pu = touch_like_pointer.clone();
    let on_pointer_up = Closure::wrap(Box::new(move |ev: PointerEvent| {
        if !alive_pu.load(Ordering::Relaxed)
            || !primary_pointer_event(&ev)
            || active_pu.get() != Some(ev.pointer_id())
        {
            return;
        }
        ev.prevent_default();
        let coords = pointer_coords(&canvas_el, &ev).or_else(|| last_pu.get());
        // A focus/capture loss can arrive during the short-touch delay.
        // Preserve the actual up coordinates for that release too.
        last_pu.set(coords);
        let delay_ms = if touch_like_pu.get() {
            touch_release_delay_ms(down_ms_pu.get())
        } else {
            0.0
        };
        release_pointer_mouse_up_after(
            canvas_el.clone(),
            machine_pu.clone(),
            alive_pu.clone(),
            active_pu.clone(),
            last_pu.clone(),
            ev.pointer_id(),
            coords,
            delay_ms,
        );
    }) as Box<dyn FnMut(PointerEvent)>);
    let _ = canvas
        .add_event_listener_with_callback("pointerup", on_pointer_up.as_ref().unchecked_ref());
    listeners.borrow_mut().push(InputListener::retain(
        canvas.as_ref(),
        "pointerup",
        on_pointer_up,
    ));

    let canvas_el = canvas.clone();
    let machine_pc = machine;
    let alive_pc = alive;
    let active_pc = active_pointer_id;
    let last_pc = last_coords;
    let on_pointer_cancel = Closure::wrap(Box::new(move |ev: PointerEvent| {
        if !alive_pc.load(Ordering::Relaxed) || active_pc.get() != Some(ev.pointer_id()) {
            return;
        }
        ev.prevent_default();
        let coords = if ev.type_() == "lostpointercapture" {
            last_pc.get()
        } else {
            pointer_coords(&canvas_el, &ev).or_else(|| last_pc.get())
        };
        active_pc.set(None);
        last_pc.set(None);
        let _ = canvas_el.release_pointer_capture(ev.pointer_id());
        if let Some((v, h)) = coords {
            machine_pc.mouse_up(v, h);
        }
    }) as Box<dyn FnMut(PointerEvent)>);
    let _ = canvas.add_event_listener_with_callback(
        "pointercancel",
        on_pointer_cancel.as_ref().unchecked_ref(),
    );
    retain_pointer_cancel(listeners, canvas.as_ref(), on_pointer_cancel);
}

fn attach_touch_input(
    canvas: &HtmlCanvasElement,
    machine: RuntimeHandle,
    alive: Arc<AtomicBool>,
    suppress_mouse_until_ms: Rc<Cell<f64>>,
    listeners: &InputListeners,
) {
    let active_touch_id: Rc<Cell<Option<i32>>> = Rc::new(Cell::new(None));
    let last_coords: Rc<Cell<Option<(i16, i16)>>> = Rc::new(Cell::new(None));
    let touch_down_ms: Rc<Cell<f64>> = Rc::new(Cell::new(0.0));

    let focus_machine = machine.clone();
    let focus_alive = alive.clone();
    let focus_active = active_touch_id.clone();
    let focus_coords = last_coords.clone();
    attach_focus_release(Some(canvas), listeners, move || {
        if let Some(identifier) = focus_active.get() {
            release_touch_mouse_up_after(
                focus_machine.clone(),
                focus_alive.clone(),
                focus_active.clone(),
                focus_coords.clone(),
                identifier,
                focus_coords.get(),
                0.0,
            );
        }
    });

    let canvas_el = canvas.clone();
    let machine_ts = machine.clone();
    let alive_ts = alive.clone();
    let active_ts = active_touch_id.clone();
    let last_ts = last_coords.clone();
    let down_ms_ts = touch_down_ms.clone();
    let suppress_ts = suppress_mouse_until_ms.clone();
    let on_touch_start = Closure::wrap(Box::new(move |ev: TouchEvent| {
        if !alive_ts.load(Ordering::Relaxed) || active_ts.get().is_some() {
            return;
        }
        let Some(touch) = first_touch(&ev.changed_touches()) else {
            return;
        };
        focus_canvas(&canvas_el);
        ev.prevent_default();
        suppress_compat_mouse(&suppress_ts);
        let m = &machine_ts;
        m.resume_audio();
        if let Some((v, h)) = touch_coords(&canvas_el, &touch) {
            active_ts.set(Some(touch.identifier()));
            last_ts.set(Some((v, h)));
            down_ms_ts.set(perf_now_ms());
            m.mouse_down(v, h);
        }
    }) as Box<dyn FnMut(TouchEvent)>);
    let _ = canvas
        .add_event_listener_with_callback("touchstart", on_touch_start.as_ref().unchecked_ref());
    listeners.borrow_mut().push(InputListener::retain(
        canvas.as_ref(),
        "touchstart",
        on_touch_start,
    ));

    let canvas_el = canvas.clone();
    let machine_tm = machine.clone();
    let alive_tm = alive.clone();
    let active_tm = active_touch_id.clone();
    let last_tm = last_coords.clone();
    let suppress_tm = suppress_mouse_until_ms.clone();
    let on_touch_move = Closure::wrap(Box::new(move |ev: TouchEvent| {
        if !alive_tm.load(Ordering::Relaxed) {
            return;
        }
        let Some(identifier) = active_tm.get() else {
            return;
        };
        let Some(touch) = touch_by_identifier(&ev.changed_touches(), identifier) else {
            return;
        };
        ev.prevent_default();
        suppress_compat_mouse(&suppress_tm);
        if let Some((v, h)) = touch_coords(&canvas_el, &touch) {
            last_tm.set(Some((v, h)));
            machine_tm.mouse_move(v, h);
        }
    }) as Box<dyn FnMut(TouchEvent)>);
    let _ = canvas
        .add_event_listener_with_callback("touchmove", on_touch_move.as_ref().unchecked_ref());
    listeners.borrow_mut().push(InputListener::retain(
        canvas.as_ref(),
        "touchmove",
        on_touch_move,
    ));

    let canvas_el = canvas.clone();
    let machine_te = machine.clone();
    let alive_te = alive.clone();
    let active_te = active_touch_id.clone();
    let last_te = last_coords.clone();
    let down_ms_te = touch_down_ms.clone();
    let suppress_te = suppress_mouse_until_ms.clone();
    let on_touch_end = Closure::wrap(Box::new(move |ev: TouchEvent| {
        if !alive_te.load(Ordering::Relaxed) {
            return;
        }
        let Some(identifier) = active_te.get() else {
            return;
        };
        let Some(touch) = touch_by_identifier(&ev.changed_touches(), identifier) else {
            return;
        };
        ev.prevent_default();
        suppress_compat_mouse(&suppress_te);
        let coords = touch_coords(&canvas_el, &touch).or_else(|| last_te.get());
        last_te.set(coords);
        release_touch_mouse_up_after(
            machine_te.clone(),
            alive_te.clone(),
            active_te.clone(),
            last_te.clone(),
            identifier,
            coords,
            touch_release_delay_ms(down_ms_te.get()),
        );
    }) as Box<dyn FnMut(TouchEvent)>);
    let _ =
        canvas.add_event_listener_with_callback("touchend", on_touch_end.as_ref().unchecked_ref());
    listeners.borrow_mut().push(InputListener::retain(
        canvas.as_ref(),
        "touchend",
        on_touch_end,
    ));

    let machine_tc = machine;
    let alive_tc = alive;
    let active_tc = active_touch_id;
    let last_tc = last_coords;
    let suppress_tc = suppress_mouse_until_ms;
    let on_touch_cancel = Closure::wrap(Box::new(move |ev: TouchEvent| {
        if !alive_tc.load(Ordering::Relaxed) {
            return;
        }
        let Some(identifier) = active_tc.get() else {
            return;
        };
        if touch_by_identifier(&ev.changed_touches(), identifier).is_none() {
            return;
        }
        ev.prevent_default();
        suppress_compat_mouse(&suppress_tc);
        let coords = last_tc.get();
        active_tc.set(None);
        last_tc.set(None);
        if let Some((v, h)) = coords {
            machine_tc.mouse_up(v, h);
        }
    }) as Box<dyn FnMut(TouchEvent)>);
    let _ = canvas
        .add_event_listener_with_callback("touchcancel", on_touch_cancel.as_ref().unchecked_ref());
    listeners.borrow_mut().push(InputListener::retain(
        canvas.as_ref(),
        "touchcancel",
        on_touch_cancel,
    ));
}

fn attach_mouse_input(
    canvas: &HtmlCanvasElement,
    machine: RuntimeHandle,
    alive: Arc<AtomicBool>,
    suppress_mouse_until_ms: Rc<Cell<f64>>,
    listeners: &InputListeners,
) {
    let last_down = Rc::new(Cell::new(None));
    let focus_down = last_down.clone();
    let focus_machine = machine.clone();
    let focus_alive = alive.clone();
    attach_focus_release(Some(canvas), listeners, move || {
        if focus_alive.load(Ordering::Relaxed) {
            if let Some((v, h)) = focus_down.take() {
                focus_machine.mouse_up(v, h);
            }
        }
    });
    // Mouse down / up / move fallback for browsers without PointerEvent.
    // Interaction-start handlers also try to resume the AudioContext: creation
    // happens inside an async chain that may have slipped outside the
    // click-gesture microtask, so browsers leave the context suspended until
    // we kick it from inside a gesture handler. Handlers short-circuit when the
    // component has unmounted while a callback was already queued.
    let canvas_el = canvas.clone();
    let machine_md = machine.clone();
    let alive_md = alive.clone();
    let suppress_md = suppress_mouse_until_ms.clone();
    let down_md = last_down.clone();
    let on_down = Closure::wrap(Box::new(move |ev: MouseEvent| {
        if !alive_md.load(Ordering::Relaxed) || mouse_is_suppressed(&suppress_md) {
            return;
        }
        focus_canvas(&canvas_el);
        ev.prevent_default();
        let m = &machine_md;
        m.resume_audio();
        if let Some((v, h)) = canvas_coords(&canvas_el, &ev) {
            down_md.set(Some((v, h)));
            m.mouse_down(v, h);
        }
    }) as Box<dyn FnMut(MouseEvent)>);
    let _ = canvas.add_event_listener_with_callback("mousedown", on_down.as_ref().unchecked_ref());
    listeners
        .borrow_mut()
        .push(InputListener::retain(canvas.as_ref(), "mousedown", on_down));

    let canvas_el = canvas.clone();
    let machine_mu = machine.clone();
    let alive_mu = alive.clone();
    let suppress_mu = suppress_mouse_until_ms.clone();
    let down_mu = last_down.clone();
    let on_up = Closure::wrap(Box::new(move |ev: MouseEvent| {
        if !alive_mu.load(Ordering::Relaxed) || mouse_is_suppressed(&suppress_mu) {
            return;
        }
        if let Some((v, h)) = canvas_coords(&canvas_el, &ev) {
            down_mu.set(None);
            machine_mu.mouse_up(v, h);
        }
    }) as Box<dyn FnMut(MouseEvent)>);
    let _ = canvas.add_event_listener_with_callback("mouseup", on_up.as_ref().unchecked_ref());
    listeners
        .borrow_mut()
        .push(InputListener::retain(canvas.as_ref(), "mouseup", on_up));

    let canvas_el = canvas.clone();
    let machine_mm = machine;
    let alive_mm = alive;
    let suppress_mm = suppress_mouse_until_ms;
    let down_mm = last_down;
    let on_move = Closure::wrap(Box::new(move |ev: MouseEvent| {
        if !alive_mm.load(Ordering::Relaxed) || mouse_is_suppressed(&suppress_mm) {
            return;
        }
        if let Some((v, h)) = canvas_coords(&canvas_el, &ev) {
            if down_mm.get().is_some() {
                down_mm.set(Some((v, h)));
            }
            machine_mm.mouse_move(v, h);
        }
    }) as Box<dyn FnMut(MouseEvent)>);
    let _ = canvas.add_event_listener_with_callback("mousemove", on_move.as_ref().unchecked_ref());
    listeners
        .borrow_mut()
        .push(InputListener::retain(canvas.as_ref(), "mousemove", on_move));
}

fn attach_mobile_controls(
    controls: &HtmlElement,
    joystick: &HtmlElement,
    settings: MobileControls,
    machine: RuntimeHandle,
    alive: Arc<AtomicBool>,
    listeners: &InputListeners,
) {
    let state = Rc::new(RefCell::new(MobileInputState::new()));
    attach_mobile_gesture_guard(controls, alive.clone(), listeners);
    attach_mobile_joystick(
        joystick,
        settings,
        machine.clone(),
        alive.clone(),
        state.clone(),
        listeners,
    );
    attach_mobile_buttons(controls, machine, alive, state, listeners);
}

fn attach_mobile_gesture_guard(
    controls: &HtmlElement,
    alive: Arc<AtomicBool>,
    listeners: &InputListeners,
) {
    let alive_ts = alive.clone();
    let on_touch_start = Closure::wrap(Box::new(move |ev: TouchEvent| {
        if alive_ts.load(Ordering::Relaxed) {
            ev.prevent_default();
        }
    }) as Box<dyn FnMut(TouchEvent)>);
    add_non_passive_touch_listener(controls, "touchstart", &on_touch_start);
    listeners.borrow_mut().push(InputListener::retain(
        controls.as_ref(),
        "touchstart",
        on_touch_start,
    ));

    let alive_tm = alive.clone();
    let on_touch_move = Closure::wrap(Box::new(move |ev: TouchEvent| {
        if alive_tm.load(Ordering::Relaxed) {
            ev.prevent_default();
        }
    }) as Box<dyn FnMut(TouchEvent)>);
    add_non_passive_touch_listener(controls, "touchmove", &on_touch_move);
    listeners.borrow_mut().push(InputListener::retain(
        controls.as_ref(),
        "touchmove",
        on_touch_move,
    ));

    let alive_te = alive.clone();
    let on_touch_end = Closure::wrap(Box::new(move |ev: TouchEvent| {
        if alive_te.load(Ordering::Relaxed) {
            ev.prevent_default();
        }
    }) as Box<dyn FnMut(TouchEvent)>);
    add_non_passive_touch_listener(controls, "touchend", &on_touch_end);
    listeners.borrow_mut().push(InputListener::retain(
        controls.as_ref(),
        "touchend",
        on_touch_end,
    ));

    let alive_tc = alive.clone();
    let on_touch_cancel = Closure::wrap(Box::new(move |ev: TouchEvent| {
        if alive_tc.load(Ordering::Relaxed) {
            ev.prevent_default();
        }
    }) as Box<dyn FnMut(TouchEvent)>);
    add_non_passive_touch_listener(controls, "touchcancel", &on_touch_cancel);
    listeners.borrow_mut().push(InputListener::retain(
        controls.as_ref(),
        "touchcancel",
        on_touch_cancel,
    ));

    attach_safari_gesture_guard(controls, "gesturestart", alive.clone(), listeners);
    attach_safari_gesture_guard(controls, "gesturechange", alive.clone(), listeners);
    attach_safari_gesture_guard(controls, "gestureend", alive, listeners);
}

fn add_non_passive_touch_listener(
    element: &HtmlElement,
    event_name: &str,
    listener: &Closure<dyn FnMut(TouchEvent)>,
) {
    let options = non_passive_listener_options();
    let _ = element.add_event_listener_with_callback_and_add_event_listener_options(
        event_name,
        listener.as_ref().unchecked_ref(),
        &options,
    );
}

fn attach_safari_gesture_guard(
    element: &HtmlElement,
    event_name: &str,
    alive: Arc<AtomicBool>,
    listeners: &InputListeners,
) {
    let on_gesture = Closure::wrap(Box::new(move |ev: Event| {
        if alive.load(Ordering::Relaxed) {
            ev.prevent_default();
        }
    }) as Box<dyn FnMut(Event)>);
    let options = non_passive_listener_options();
    let _ = element.add_event_listener_with_callback_and_add_event_listener_options(
        event_name,
        on_gesture.as_ref().unchecked_ref(),
        &options,
    );
    listeners.borrow_mut().push(InputListener::retain(
        element.as_ref(),
        event_name,
        on_gesture,
    ));
}

fn non_passive_listener_options() -> AddEventListenerOptions {
    let options = AddEventListenerOptions::new();
    options.set_passive(false);
    options
}

fn attach_mobile_joystick(
    joystick: &HtmlElement,
    settings: MobileControls,
    machine: RuntimeHandle,
    alive: Arc<AtomicBool>,
    state: Rc<RefCell<MobileInputState>>,
    listeners: &InputListeners,
) {
    let active_pointer_id: Rc<Cell<Option<i32>>> = Rc::new(Cell::new(None));

    let focus_active = active_pointer_id.clone();
    let focus_state = state.clone();
    let focus_joystick = joystick.clone();
    let focus_machine = machine.clone();
    let focus_alive = alive.clone();
    attach_focus_release(None, listeners, move || {
        if focus_alive.load(Ordering::Relaxed) {
            if let Some(id) = focus_active.take() {
                let _ = focus_joystick.release_pointer_capture(id);
            }
            set_mobile_joystick_vector(&focus_joystick, 0.0, 0.0);
            focus_state
                .borrow_mut()
                .set_joystick_keys(&focus_machine, &[]);
        }
    });

    let joystick_el = joystick.clone();
    let machine_pd = machine.clone();
    let alive_pd = alive.clone();
    let state_pd = state.clone();
    let active_pd = active_pointer_id.clone();
    let on_pointer_down = Closure::wrap(Box::new(move |ev: PointerEvent| {
        if !alive_pd.load(Ordering::Relaxed)
            || !mobile_control_pointer_event(&ev)
            || ev.button() != 0
            || active_pd.get().is_some()
        {
            return;
        }
        ev.prevent_default();
        active_pd.set(Some(ev.pointer_id()));
        let _ = joystick_el.set_pointer_capture(ev.pointer_id());
        joystick_el.class_list().add_1("is-active").ok();
        machine_pd.resume_audio();
        update_mobile_joystick(
            &joystick_el,
            settings,
            &state_pd,
            &machine_pd,
            ev.client_x(),
            ev.client_y(),
        );
    }) as Box<dyn FnMut(PointerEvent)>);
    let _ = joystick
        .add_event_listener_with_callback("pointerdown", on_pointer_down.as_ref().unchecked_ref());
    listeners.borrow_mut().push(InputListener::retain(
        joystick.as_ref(),
        "pointerdown",
        on_pointer_down,
    ));

    let joystick_el = joystick.clone();
    let machine_pm = machine.clone();
    let alive_pm = alive.clone();
    let state_pm = state.clone();
    let active_pm = active_pointer_id.clone();
    let on_pointer_move = Closure::wrap(Box::new(move |ev: PointerEvent| {
        if !alive_pm.load(Ordering::Relaxed) || active_pm.get() != Some(ev.pointer_id()) {
            return;
        }
        ev.prevent_default();
        update_mobile_joystick(
            &joystick_el,
            settings,
            &state_pm,
            &machine_pm,
            ev.client_x(),
            ev.client_y(),
        );
    }) as Box<dyn FnMut(PointerEvent)>);
    let _ = joystick
        .add_event_listener_with_callback("pointermove", on_pointer_move.as_ref().unchecked_ref());
    listeners.borrow_mut().push(InputListener::retain(
        joystick.as_ref(),
        "pointermove",
        on_pointer_move,
    ));

    let joystick_el = joystick.clone();
    let machine_pu = machine.clone();
    let alive_pu = alive.clone();
    let state_pu = state.clone();
    let active_pu = active_pointer_id.clone();
    let on_pointer_up = Closure::wrap(Box::new(move |ev: PointerEvent| {
        if !alive_pu.load(Ordering::Relaxed) || active_pu.get() != Some(ev.pointer_id()) {
            return;
        }
        ev.prevent_default();
        active_pu.set(None);
        let _ = joystick_el.release_pointer_capture(ev.pointer_id());
        joystick_el.class_list().remove_1("is-active").ok();
        set_mobile_joystick_vector(&joystick_el, 0.0, 0.0);
        state_pu.borrow_mut().set_joystick_keys(&machine_pu, &[]);
    }) as Box<dyn FnMut(PointerEvent)>);
    let _ = joystick
        .add_event_listener_with_callback("pointerup", on_pointer_up.as_ref().unchecked_ref());
    listeners.borrow_mut().push(InputListener::retain(
        joystick.as_ref(),
        "pointerup",
        on_pointer_up,
    ));

    let joystick_el = joystick.clone();
    let machine_pc = machine;
    let alive_pc = alive;
    let state_pc = state;
    let active_pc = active_pointer_id;
    let on_pointer_cancel = Closure::wrap(Box::new(move |ev: PointerEvent| {
        if !alive_pc.load(Ordering::Relaxed) || active_pc.get() != Some(ev.pointer_id()) {
            return;
        }
        ev.prevent_default();
        active_pc.set(None);
        let _ = joystick_el.release_pointer_capture(ev.pointer_id());
        joystick_el.class_list().remove_1("is-active").ok();
        set_mobile_joystick_vector(&joystick_el, 0.0, 0.0);
        state_pc.borrow_mut().set_joystick_keys(&machine_pc, &[]);
    }) as Box<dyn FnMut(PointerEvent)>);
    let _ = joystick.add_event_listener_with_callback(
        "pointercancel",
        on_pointer_cancel.as_ref().unchecked_ref(),
    );
    retain_pointer_cancel(listeners, joystick.as_ref(), on_pointer_cancel);
}

fn attach_mobile_buttons(
    controls: &HtmlElement,
    machine: RuntimeHandle,
    alive: Arc<AtomicBool>,
    state: Rc<RefCell<MobileInputState>>,
    listeners: &InputListeners,
) {
    let active_buttons: Rc<RefCell<Vec<(i32, &'static str, HtmlElement)>>> =
        Rc::new(RefCell::new(Vec::new()));

    let focus_active = active_buttons.clone();
    let focus_state = state.clone();
    let focus_machine = machine.clone();
    let focus_alive = alive.clone();
    attach_focus_release(None, listeners, move || {
        if focus_alive.load(Ordering::Relaxed) {
            release_active_mobile_buttons(&focus_active, &focus_state, &focus_machine);
        }
    });

    let controls_pd = controls.clone();
    let machine_pd = machine.clone();
    let alive_pd = alive.clone();
    let state_pd = state.clone();
    let active_pd = active_buttons.clone();
    let on_pointer_down = Closure::wrap(Box::new(move |ev: PointerEvent| {
        if !alive_pd.load(Ordering::Relaxed)
            || !mobile_control_pointer_event(&ev)
            || ev.button() != 0
        {
            return;
        }
        if let Some(tab) = mobile_group_tab_event_target(&ev) {
            ev.prevent_default();
            switch_mobile_button_group(&controls_pd, &tab, &active_pd, &state_pd, &machine_pd);
            return;
        }
        let Some(button) = mobile_button_event_target(&ev) else {
            return;
        };
        let Some(key) = button
            .get_attribute("data-mobile-control-key")
            .and_then(mobile_control_key_name)
        else {
            return;
        };
        ev.prevent_default();
        button.class_list().add_1("is-active").ok();
        let _ = button.set_pointer_capture(ev.pointer_id());
        active_pd
            .borrow_mut()
            .push((ev.pointer_id(), key, button.clone()));
        let m = &machine_pd;
        m.resume_audio();
        state_pd.borrow_mut().press_button_key(m, key);
    }) as Box<dyn FnMut(PointerEvent)>);
    let _ = controls
        .add_event_listener_with_callback("pointerdown", on_pointer_down.as_ref().unchecked_ref());
    listeners.borrow_mut().push(InputListener::retain(
        controls.as_ref(),
        "pointerdown",
        on_pointer_down,
    ));

    let machine_pu = machine.clone();
    let alive_pu = alive.clone();
    let state_pu = state.clone();
    let active_pu = active_buttons.clone();
    let on_pointer_up = Closure::wrap(Box::new(move |ev: PointerEvent| {
        if !alive_pu.load(Ordering::Relaxed) {
            return;
        }
        if let Some((key, button)) = take_active_mobile_button(&active_pu, ev.pointer_id()) {
            ev.prevent_default();
            button.class_list().remove_1("is-active").ok();
            let _ = button.release_pointer_capture(ev.pointer_id());
            state_pu.borrow_mut().release_button_key(&machine_pu, key);
        }
    }) as Box<dyn FnMut(PointerEvent)>);
    let _ = controls
        .add_event_listener_with_callback("pointerup", on_pointer_up.as_ref().unchecked_ref());
    listeners.borrow_mut().push(InputListener::retain(
        controls.as_ref(),
        "pointerup",
        on_pointer_up,
    ));

    let machine_pc = machine;
    let alive_pc = alive;
    let state_pc = state;
    let active_pc = active_buttons;
    let on_pointer_cancel = Closure::wrap(Box::new(move |ev: PointerEvent| {
        if !alive_pc.load(Ordering::Relaxed) {
            return;
        }
        if let Some((key, button)) = take_active_mobile_button(&active_pc, ev.pointer_id()) {
            ev.prevent_default();
            button.class_list().remove_1("is-active").ok();
            let _ = button.release_pointer_capture(ev.pointer_id());
            state_pc.borrow_mut().release_button_key(&machine_pc, key);
        }
    }) as Box<dyn FnMut(PointerEvent)>);
    let _ = controls.add_event_listener_with_callback(
        "pointercancel",
        on_pointer_cancel.as_ref().unchecked_ref(),
    );
    retain_pointer_cancel(listeners, controls.as_ref(), on_pointer_cancel);
}

fn update_mobile_joystick(
    joystick: &HtmlElement,
    settings: MobileControls,
    state: &Rc<RefCell<MobileInputState>>,
    machine: &RuntimeHandle,
    client_x: i32,
    client_y: i32,
) {
    let rect = joystick.get_bounding_client_rect();
    let radius = rect.width().min(rect.height()) * 0.5;
    if radius <= 0.0 {
        return;
    }
    let dx = client_x as f64 - (rect.x() + rect.width() * 0.5);
    let dy = client_y as f64 - (rect.y() + rect.height() * 0.5);
    let distance = (dx * dx + dy * dy).sqrt();
    let clamped_distance = distance.min(radius);
    let scale = if distance > 0.0 {
        clamped_distance / distance
    } else {
        0.0
    };
    let nx = (dx * scale) / radius;
    let ny = (dy * scale) / radius;
    set_mobile_joystick_vector(joystick, nx, ny);

    let dead_zone = 0.32;
    let mut keys = Vec::with_capacity(2);
    if nx < -dead_zone {
        keys.push(settings.joystick.left);
    } else if nx > dead_zone {
        keys.push(settings.joystick.right);
    }
    if ny < -dead_zone {
        keys.push(settings.joystick.up);
    } else if ny > dead_zone {
        keys.push(settings.joystick.down);
    }
    state.borrow_mut().set_joystick_keys(machine, &keys);
}

fn set_mobile_joystick_vector(joystick: &HtmlElement, x: f64, y: f64) {
    let style = joystick.style();
    let _ = style.set_property("--stick-x", &format!("{:.3}", x.clamp(-1.0, 1.0)));
    let _ = style.set_property("--stick-y", &format!("{:.3}", y.clamp(-1.0, 1.0)));
}

fn mobile_button_event_target(ev: &PointerEvent) -> Option<HtmlElement> {
    ev.target()?.dyn_into::<HtmlElement>().ok().and_then(|el| {
        if el.has_attribute("data-mobile-control-key") {
            Some(el)
        } else {
            None
        }
    })
}

fn mobile_group_tab_event_target(ev: &PointerEvent) -> Option<HtmlElement> {
    ev.target()?.dyn_into::<HtmlElement>().ok().and_then(|el| {
        if el.has_attribute("data-mobile-control-group-tab") {
            Some(el)
        } else {
            None
        }
    })
}

fn switch_mobile_button_group(
    controls: &HtmlElement,
    tab: &HtmlElement,
    active_buttons: &Rc<RefCell<Vec<(i32, &'static str, HtmlElement)>>>,
    state: &Rc<RefCell<MobileInputState>>,
    machine: &RuntimeHandle,
) {
    let Some(next_group) = tab.get_attribute("data-mobile-control-group-tab") else {
        return;
    };

    release_active_mobile_buttons(active_buttons, state, machine);

    if let Ok(Some(active_tab)) = controls.query_selector(".mobile-button-tab.is-active") {
        active_tab.class_list().remove_1("is-active").ok();
        let _ = active_tab.set_attribute("aria-pressed", "false");
    }

    tab.class_list().add_1("is-active").ok();
    let _ = tab.set_attribute("aria-pressed", "true");

    if let Ok(Some(active_group)) = controls.query_selector(".mobile-buttons.is-active") {
        active_group.class_list().remove_1("is-active").ok();
    }

    let group_selector = format!("[data-mobile-control-group=\"{next_group}\"]");
    if let Ok(Some(next_group)) = controls.query_selector(&group_selector) {
        next_group.class_list().add_1("is-active").ok();
    }
}

fn release_active_mobile_buttons(
    active_buttons: &Rc<RefCell<Vec<(i32, &'static str, HtmlElement)>>>,
    state: &Rc<RefCell<MobileInputState>>,
    machine: &RuntimeHandle,
) {
    let pressed = active_buttons.borrow_mut().drain(..).collect::<Vec<_>>();
    if pressed.is_empty() {
        return;
    }

    let m = machine;
    let mut input = state.borrow_mut();
    for (pointer_id, key, button) in pressed {
        button.class_list().remove_1("is-active").ok();
        let _ = button.release_pointer_capture(pointer_id);
        input.release_button_key(m, key);
    }
}

fn take_active_mobile_button(
    active: &Rc<RefCell<Vec<(i32, &'static str, HtmlElement)>>>,
    pointer_id: i32,
) -> Option<(&'static str, HtmlElement)> {
    let mut active = active.borrow_mut();
    let index = active.iter().position(|(id, _, _)| *id == pointer_id)?;
    let (_, key, element) = active.remove(index);
    Some((key, element))
}

#[derive(Default)]
struct MobileInputState {
    joystick_keys: Vec<&'static str>,
    button_keys: Vec<&'static str>,
    sent_keys: Vec<&'static str>,
}

impl MobileInputState {
    fn new() -> Self {
        Self::default()
    }

    fn set_joystick_keys(&mut self, machine: &RuntimeHandle, keys: &[&'static str]) {
        self.joystick_keys = dedup_mobile_keys(keys);
        self.flush(machine);
    }

    fn press_button_key(&mut self, machine: &RuntimeHandle, key: &'static str) {
        if !self.button_keys.contains(&key) {
            self.button_keys.push(key);
            self.flush(machine);
        }
    }

    fn release_button_key(&mut self, machine: &RuntimeHandle, key: &'static str) {
        self.button_keys.retain(|held| *held != key);
        self.flush(machine);
    }

    fn flush(&mut self, machine: &RuntimeHandle) {
        let mut next = self.joystick_keys.clone();
        for key in &self.button_keys {
            if !next.contains(key) {
                next.push(key);
            }
        }

        for key in self.sent_keys.iter().copied() {
            if !next.contains(&key) {
                if let Some((mac_key, char_code)) = mobile_key_code(key) {
                    machine.mobile_key(false, mac_key, char_code);
                }
            }
        }
        for key in next.iter().copied() {
            if !self.sent_keys.contains(&key) {
                if let Some((mac_key, char_code)) = mobile_key_code(key) {
                    machine.mobile_key(true, mac_key, char_code);
                }
            }
        }
        self.sent_keys = next;
    }
}

fn dedup_mobile_keys(keys: &[&'static str]) -> Vec<&'static str> {
    let mut out = Vec::with_capacity(keys.len());
    for key in keys {
        if !out.contains(key) {
            out.push(*key);
        }
    }
    out
}

fn mobile_control_key_name(key: String) -> Option<&'static str> {
    match key.as_str() {
        "ArrowUp" => Some("ArrowUp"),
        "ArrowDown" => Some("ArrowDown"),
        "ArrowLeft" => Some("ArrowLeft"),
        "ArrowRight" => Some("ArrowRight"),
        "Space" => Some("Space"),
        "Enter" => Some("Enter"),
        "Tab" => Some("Tab"),
        "Escape" => Some("Escape"),
        "Shift" => Some("Shift"),
        "None" => Some("None"),
        "[" => Some("["),
        "]" => Some("]"),
        "\\" => Some("\\"),
        "a" | "A" => Some("a"),
        "b" | "B" => Some("b"),
        "c" | "C" => Some("c"),
        "d" | "D" => Some("d"),
        "e" | "E" => Some("e"),
        "f" | "F" => Some("f"),
        "g" | "G" => Some("g"),
        "h" | "H" => Some("h"),
        "i" | "I" => Some("i"),
        "j" | "J" => Some("j"),
        "k" | "K" => Some("k"),
        "l" | "L" => Some("l"),
        "m" | "M" => Some("m"),
        "n" | "N" => Some("n"),
        "o" | "O" => Some("o"),
        "p" | "P" => Some("p"),
        "q" | "Q" => Some("q"),
        "r" | "R" => Some("r"),
        "s" | "S" => Some("s"),
        "t" | "T" => Some("t"),
        "u" | "U" => Some("u"),
        "v" | "V" => Some("v"),
        "w" | "W" => Some("w"),
        "x" | "X" => Some("x"),
        "y" | "Y" => Some("y"),
        "z" | "Z" => Some("z"),
        "0" => Some("0"),
        "1" => Some("1"),
        "2" => Some("2"),
        "3" => Some("3"),
        "4" => Some("4"),
        "5" => Some("5"),
        "6" => Some("6"),
        "7" => Some("7"),
        "8" => Some("8"),
        "9" => Some("9"),
        _ => None,
    }
}

fn mobile_key_code(key: &str) -> Option<(u8, u8)> {
    match key {
        "Space" => map_key(" ", "Space"),
        "Enter" => map_key("Enter", "Enter"),
        "Tab" => map_key("Tab", "Tab"),
        "Escape" => map_key("Escape", "Escape"),
        "Shift" => map_key("Shift", "ShiftLeft"),
        "None" => None,
        "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight" => map_key(key, key),
        single if single.len() == 1 => map_key(single, single),
        _ => None,
    }
}

fn attach_debug_toggle(
    canvas: &HtmlCanvasElement,
    alive: Arc<AtomicBool>,
    debug_visible: RwSignal<bool>,
    listeners: &InputListeners,
) {
    let on_key_down = Closure::wrap(Box::new(move |ev: KeyboardEvent| {
        if !alive.load(Ordering::Relaxed) || ev.code() != "F3" || ev.repeat() {
            return;
        }
        ev.prevent_default();
        debug_visible.update(|visible| *visible = !*visible);
    }) as Box<dyn FnMut(KeyboardEvent)>);
    let _ =
        canvas.add_event_listener_with_callback("keydown", on_key_down.as_ref().unchecked_ref());
    listeners.borrow_mut().push(InputListener::retain(
        canvas.as_ref(),
        "keydown",
        on_key_down,
    ));
}

fn perf_now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}

fn focus_canvas(canvas: &HtmlCanvasElement) {
    if let Some(element) = canvas.dyn_ref::<HtmlElement>() {
        let _ = element.focus();
    }
}

fn primary_pointer_event(ev: &PointerEvent) -> bool {
    primary_pointer(&ev.pointer_type(), ev.is_primary())
}

fn primary_pointer(pointer_type: &str, is_primary: bool) -> bool {
    pointer_type == "mouse" || is_primary
}

fn mobile_control_pointer_event(ev: &PointerEvent) -> bool {
    mobile_control_pointer(&ev.pointer_type(), ev.is_primary())
}

fn mobile_control_pointer(pointer_type: &str, is_primary: bool) -> bool {
    pointer_type != "mouse" || is_primary
}

fn pointer_coords(canvas: &HtmlCanvasElement, ev: &PointerEvent) -> Option<(i16, i16)> {
    canvas_coords_from_client(canvas, ev.client_x(), ev.client_y())
}

fn touch_coords(canvas: &HtmlCanvasElement, touch: &Touch) -> Option<(i16, i16)> {
    canvas_coords_from_client(canvas, touch.client_x(), touch.client_y())
}

fn canvas_coords(canvas: &HtmlCanvasElement, ev: &MouseEvent) -> Option<(i16, i16)> {
    canvas_coords_from_client(canvas, ev.client_x(), ev.client_y())
}

fn logical_pointer_position(
    position: (f64, f64),
    css_size: (f64, f64),
    backing: (u32, u32),
    scale: f64,
) -> (i16, i16) {
    let x = position.0 * backing.0 as f64 / scale / css_size.0;
    let y = position.1 * backing.1 as f64 / scale / css_size.1;
    (y.round() as i16, x.round() as i16)
}

fn canvas_backing_scale(canvas: &HtmlCanvasElement) -> u32 {
    canvas
        .get_attribute("data-output-scale")
        .and_then(|value| value.parse().ok())
        .unwrap_or(1)
        .max(1)
}

fn canvas_output_scale(canvas: &HtmlCanvasElement, logical: (u32, u32)) -> u32 {
    let rect = canvas.get_bounding_client_rect();
    let dpr = web_sys::window().map_or(1.0, |window| window.device_pixel_ratio());
    systemless::display::outline_output_scale(
        logical,
        (
            (rect.width() * dpr).ceil() as u32,
            (rect.height() * dpr).ceil() as u32,
        ),
    )
}

fn canvas_coords_from_client(
    canvas: &HtmlCanvasElement,
    client_x: i32,
    client_y: i32,
) -> Option<(i16, i16)> {
    let rect = canvas.get_bounding_client_rect();
    let rw = rect.width();
    let rh = rect.height();
    if rw <= 0.0 || rh <= 0.0 {
        return None;
    }
    let scale = canvas_backing_scale(canvas) as f64;
    Some(logical_pointer_position(
        (client_x as f64 - rect.x(), client_y as f64 - rect.y()),
        (rw, rh),
        (canvas.width(), canvas.height()),
        scale,
    ))
}

fn first_touch(touches: &TouchList) -> Option<Touch> {
    touches.item(0)
}

fn touch_by_identifier(touches: &TouchList, identifier: i32) -> Option<Touch> {
    (0..touches.length())
        .filter_map(|index| touches.item(index))
        .find(|touch| touch.identifier() == identifier)
}

fn suppress_compat_mouse(suppress_mouse_until_ms: &Cell<f64>) {
    suppress_mouse_until_ms.set(perf_now_ms() + 800.0);
}

fn mouse_is_suppressed(suppress_mouse_until_ms: &Cell<f64>) -> bool {
    perf_now_ms() < suppress_mouse_until_ms.get()
}

fn touch_release_delay_ms(down_ms: f64) -> f64 {
    (MIN_TOUCH_PRESS_MS - (perf_now_ms() - down_ms)).max(0.0)
}

fn release_pointer_mouse_up_after(
    canvas: HtmlCanvasElement,
    machine: RuntimeHandle,
    alive: Arc<AtomicBool>,
    active_pointer_id: Rc<Cell<Option<i32>>>,
    last_coords: Rc<Cell<Option<(i16, i16)>>>,
    pointer_id: i32,
    coords: Option<(i16, i16)>,
    delay_ms: f64,
) {
    let release = move || {
        if active_pointer_id.get() != Some(pointer_id) {
            return;
        }
        active_pointer_id.set(None);
        last_coords.set(None);
        let _ = canvas.release_pointer_capture(pointer_id);
        if !alive.load(Ordering::Relaxed) {
            return;
        }
        if let Some((v, h)) = coords {
            machine.mouse_up(v, h);
        }
    };

    if delay_ms <= 0.0 {
        release();
        return;
    }

    let callback = Closure::once(release);
    if let Some(window) = web_sys::window() {
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            callback.as_ref().unchecked_ref(),
            delay_ms.ceil() as i32,
        );
        callback.forget();
    }
}

fn release_touch_mouse_up_after(
    machine: RuntimeHandle,
    alive: Arc<AtomicBool>,
    active_touch_id: Rc<Cell<Option<i32>>>,
    last_coords: Rc<Cell<Option<(i16, i16)>>>,
    identifier: i32,
    coords: Option<(i16, i16)>,
    delay_ms: f64,
) {
    let release = move || {
        if active_touch_id.get() != Some(identifier) {
            return;
        }
        active_touch_id.set(None);
        last_coords.set(None);
        if !alive.load(Ordering::Relaxed) {
            return;
        }
        if let Some((v, h)) = coords {
            machine.mouse_up(v, h);
        }
    };

    if delay_ms <= 0.0 {
        release();
        return;
    }

    let callback = Closure::once(release);
    if let Some(window) = web_sys::window() {
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            callback.as_ref().unchecked_ref(),
            delay_ms.ceil() as i32,
        );
        callback.forget();
    }
}

fn mapped_key(key: &str, code: &str, mappings: &[(&str, &str)]) -> Option<(u8, u8)> {
    let canonical = if key == " " { "Space" } else { key };
    match mappings.iter().find(|(from, _)| *from == canonical) {
        Some((_, to)) => mobile_key_code(to),
        None => map_key(key, code),
    }
}

fn map_key(key: &str, code: &str) -> Option<(u8, u8)> {
    // Returns (mac_adb_keycode, char_code) for the common keys a player needs.
    // Mac keycodes from Inside Macintosh Vol V — Appendix C (ADB keyboard).
    match code {
        "NumpadDecimal" => return Some((0x41, 0)),
        "NumpadMultiply" => return Some((0x43, 0)),
        "NumpadAdd" => return Some((0x45, 0)),
        "NumpadDivide" => return Some((0x4B, 0)),
        "NumpadEnter" => return Some((0x4C, 0x0D)),
        "NumpadSubtract" => return Some((0x4E, 0)),
        "NumpadEqual" => return Some((0x51, 0)),
        "Numpad0" => return Some((0x52, b'0')),
        "Numpad1" => return Some((0x53, b'1')),
        "Numpad2" => return Some((0x54, b'2')),
        "Numpad3" => return Some((0x55, b'3')),
        "Numpad4" => return Some((0x56, b'4')),
        "Numpad5" => return Some((0x57, b'5')),
        "Numpad6" => return Some((0x58, b'6')),
        "Numpad7" => return Some((0x59, b'7')),
        "Numpad8" => return Some((0x5B, b'8')),
        "Numpad9" => return Some((0x5C, b'9')),
        _ => {}
    }

    match key {
        "ArrowLeft" => return Some((0x7B, 0x1C)),
        "ArrowRight" => return Some((0x7C, 0x1D)),
        "ArrowDown" => return Some((0x7D, 0x1F)),
        "ArrowUp" => return Some((0x7E, 0x1E)),
        _ => {}
    }

    match key {
        " " => Some((0x31, 0x20)),
        "Shift" if code == "ShiftRight" => Some((0x3C, 0)),
        "Shift" => Some((0x38, 0)),
        "Enter" => Some((0x24, 0x0D)),
        "Escape" => Some((0x35, 0x1B)),
        "Tab" => Some((0x30, 0x09)),
        "Backspace" => Some((0x33, 0x08)),
        k if k.len() == 1 => {
            let c = k.as_bytes()[0];
            if c.is_ascii_alphanumeric()
                || matches!(c, b'.' | b',' | b'-' | b'=' | b'[' | b']' | b'\\')
            {
                let lower = c.to_ascii_lowercase();
                let mac = match lower {
                    b'a' => 0x00,
                    b'b' => 0x0B,
                    b'c' => 0x08,
                    b'd' => 0x02,
                    b'e' => 0x0E,
                    b'f' => 0x03,
                    b'g' => 0x05,
                    b'h' => 0x04,
                    b'i' => 0x22,
                    b'j' => 0x26,
                    b'k' => 0x28,
                    b'l' => 0x25,
                    b'm' => 0x2E,
                    b'n' => 0x2D,
                    b'o' => 0x1F,
                    b'p' => 0x23,
                    b'q' => 0x0C,
                    b'r' => 0x0F,
                    b's' => 0x01,
                    b't' => 0x11,
                    b'u' => 0x20,
                    b'v' => 0x09,
                    b'w' => 0x0D,
                    b'x' => 0x07,
                    b'y' => 0x10,
                    b'z' => 0x06,
                    b'0' => 0x1D,
                    b'1' => 0x12,
                    b'2' => 0x13,
                    b'3' => 0x14,
                    b'4' => 0x15,
                    b'5' => 0x17,
                    b'6' => 0x16,
                    b'7' => 0x1A,
                    b'8' => 0x1C,
                    b'9' => 0x19,
                    b'=' => 0x18,
                    b'.' => 0x2F,
                    b',' => 0x2B,
                    b'-' => 0x1B,
                    b'[' => 0x21,
                    b']' => 0x1E,
                    b'\\' => 0x2A,
                    _ => return None,
                };
                Some((mac, c))
            } else {
                None
            }
        }
        _ => None,
    }
}
