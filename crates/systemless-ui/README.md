# Systemless UI

An independent egui design-system crate. It has no emulator dependency. The crate provides semantic light/dark themes, density and reduced-motion settings, styled components, docking and validated workspace persistence.

The old custom retained UI runtime has been replaced with egui. Its APIs and test results do not describe this implementation. This branch is under development and must remain unmerged until the Systemless integration is verified.

## Standalone

Requires Rust 1.95. egui 0.36.2, egui_dock 0.21.1 and egui-wgpu 0.36.2 share compatible types and wgpu 30.

```sh
cargo run -p systemless-ui --features desktop --example gallery
cargo run -p systemless-ui --features desktop --example hello
```

Apply `Theme::apply` to an egui context. Use ordinary egui widgets and `components` compositions. `dialog::form` renders text, checkbox/radio choices and actions from immutable application snapshots. `dialog::show` adds host-owned editor drafts through `EditorField`, returning text/selection/focus changes alongside actions. Hosts must synchronize edits before applying an action. Popup fields use `FieldKind::Popup` with borrowed `Choice` items; `FormResponse::values` returns field IDs and selected values separately from button actions, including reselection of the current value. Synchronize edits before applying these selection commands too. `FieldKind::Range` and the independent `range::show` component provide labelled signed ranges with a one-shot focus request. Pointer drags commit on release; keyboard changes commit immediately. Source changes, disabled fields, Escape and interrupted drags cancel the draft. Control selections finish before a simultaneous default action; cancellation discards uncommitted control changes. `editor::show` also works independently, with single-line or multiline editing, Unicode character selections, stable focus and accessible labels; `file_picker` supplies virtual file navigation while the host owns I/O. `FieldKind::Embedded` shows a live region of another texture, such as content another renderer draws, at its own size; clicks on it, and on text and images when `Form::report_clicks` is set, arrive in `FormResponse::points` as fractions of the field. `workspace::show` styles an application-owned `egui_dock::DockState` with a `TabViewer`. Docking supports split layouts, movable tabs and floating in-app windows.

`window::show` provides independent nonmodal windows with application-owned `WindowState`. It retains outer geometry across hidden frames and context recreation, supports dragging/resizing, and reports activation and close intents. The caller controls lifetime and applies `WindowState::request_front` for an external activation. Keep one state and stable ID per logical window. When rendering multiple forms, use `dialog::show_with_shortcuts` and grant default/cancel keys only to the active form. These components do not inject application events. The gallery demonstrates two independent windows alongside the docked workspace.

`workspace::save` and `restore` preserve split ratios, tab ordering, active tabs and the focused pane. For rendered floating-window geometry and stacking, record each tab button’s `Response::layer_id` in `TabViewer::on_tab_button`, then call `workspace::save_with_layers(&dock, &context, |tab| layers.get(tab).copied())`. Refresh that map each frame. This captures the visible back-to-front order independently of keyboard focus. Restore saved layouts before drawing their first frame in a fresh egui context; existing window layers otherwise retain their current stacking. Version-2 snapshots store focus by stable pane identity; restoration also reads version-1 layouts. Restoration validates size, depth, identities, ratios and bounds before constructing the docking tree, and clamps windows to the visible workspace. Application code must validate saved pane IDs against its own catalogue. Collapsed state and separate native windows are not persisted. Structural saves without rendered layer observations use fallback geometry when the docking backend has no window rectangle.

## Embedding

Systemless owns its winit event loop, GPU device, queue and presentation lifecycle. Integrate `egui-winit` for platform input/output and the re-exported `egui_wgpu` renderer for composition. eframe is only the optional standalone host. The design-system crate does not own emulator execution, input routing or GPU submission.

Features:

- No default features: themes, components, docking and persistence.
- `gpu` (default): the compatible egui-wgpu renderer and wgpu types.
- `desktop`: eframe for standalone consumers.

```sh
cargo test --locked -p systemless-ui --no-default-features
cargo clippy --locked -p systemless-ui --all-features --all-targets -- -D warnings
```

See [DEVELOPMENT.md](DEVELOPMENT.md) for current integration acceptance criteria.

Accessibility trees and actions are available through egui. The standalone regressions check labelled controls, semantic button activation, and menu semantics (menu items, checkable menu items with their state, and `menu::mark_menu_title` for titles that open menus) through AccessKit; native platform adapters belong to the embedding application.
