use systemless_ui::{
    egui,
    egui_dock::{DockState, NodeIndex, TabIndex},
    workspace, Theme,
};
fn viewport() -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.))
}
#[test]
fn preserves_split_structure_tab_order_and_active_tabs() {
    let mut state = DockState::new(vec![1u64, 2]);
    state.main_surface_mut()[NodeIndex::root()]
        .get_leaf_mut()
        .unwrap()
        .active = TabIndex(1);
    state
        .main_surface_mut()
        .split_right(NodeIndex::root(), 0.65, vec![3]);
    let saved = workspace::save(&state).unwrap();
    let restored: DockState<u64> = workspace::restore(&saved, viewport()).unwrap();
    assert_eq!(workspace::save(&restored).unwrap(), saved);
}
#[test]
fn rejects_invalid_layouts_before_building_trees() {
    for data in [
        r#"{"version":3,"main":null,"windows":[]}"#,
        r#"{"version":1,"main":{"kind":"Tabs","tabs":[1,1],"active":0},"windows":[]}"#,
        r#"{"version":1,"main":{"kind":"Tabs","tabs":[1],"active":4},"windows":[]}"#,
    ] {
        assert!(workspace::restore::<u64>(data, viewport()).is_err());
    }
}
#[test]
fn empty_and_floating_workspaces_restore() {
    let mut state = DockState::<u64>::new(vec![]);
    let empty = workspace::save(&state).unwrap();
    assert!(workspace::restore::<u64>(&empty, viewport()).is_ok());
    state.add_window(vec![42]);
    let saved = workspace::save(&state).unwrap();
    let restored = workspace::restore::<u64>(&saved, viewport()).unwrap();
    assert_eq!(
        restored
            .iter_all_tabs()
            .map(|(_, tab)| *tab)
            .collect::<Vec<_>>(),
        vec![42]
    );
}
#[test]
fn both_themes_produce_a_frame_and_reduced_motion_is_respected() {
    let ctx = egui::Context::default();
    for appearance in [
        systemless_ui::Appearance::Light,
        systemless_ui::Appearance::Dark,
    ] {
        let mut theme = Theme::new(appearance);
        theme.reduced_motion = true;
        theme.apply(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert_eq!(ui.style().animation_time, 0.);
            ui.label("Hello");
        });
        assert!(!output.shapes.is_empty());
        output.textures_delta.clear();
    }
}

#[test]
fn restores_focused_pane_across_split_and_floating_surfaces() {
    for floating in [false, true] {
        let mut state = DockState::new(vec![1u64]);
        state
            .main_surface_mut()
            .split_right(NodeIndex::root(), 0.65, vec![2, 3]);
        let discarded = state.add_window(vec![4]);
        state.add_window(vec![5, 6]);
        state.remove_surface(discarded);
        let target = if floating { 6 } else { 3 };
        let path = state.find_tab(&target).unwrap();
        state.set_active_tab(path).unwrap();
        state.set_focused_node_and_surface(path.node_path());
        let saved = workspace::save(&state).unwrap();
        let mut restored: DockState<u64> = workspace::restore(&saved, viewport()).unwrap();
        assert_eq!(*restored.find_active_focused().unwrap().1, target);
        assert_eq!(workspace::save(&restored).unwrap(), saved);
        struct Viewer;
        impl systemless_ui::egui_dock::TabViewer for Viewer {
            type Tab = u64;
            fn id(&mut self, tab: &mut u64) -> egui::Id {
                egui::Id::new(*tab)
            }
            fn title(&mut self, tab: &mut u64) -> egui::WidgetText {
                format!("Pane {tab}").into()
            }
            fn ui(&mut self, ui: &mut egui::Ui, tab: &mut u64) {
                ui.label(format!("Pane {tab}"));
            }
        }
        let context = egui::Context::default();
        let theme = Theme::new(systemless_ui::Appearance::Dark);
        theme.apply(&context);
        for _ in 0..2 {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(viewport()),
                    ..Default::default()
                },
                |ui| workspace::show(ui, &theme, &mut restored, &mut Viewer),
            );
            output.textures_delta.clear();
            assert_eq!(*restored.find_active_focused().unwrap().1, target);
        }
    }
}

#[test]
fn reads_legacy_layouts_and_rejects_missing_focus_targets() {
    let legacy = r#"{"version":1,"main":{"kind":"Tabs","tabs":[1,2],"active":1},"windows":[]}"#;
    let mut restored: DockState<u64> = workspace::restore(legacy, viewport()).unwrap();
    assert!(restored.find_active_focused().is_none());
    assert_eq!(
        restored.main_surface()[NodeIndex::root()]
            .get_leaf()
            .unwrap()
            .active,
        TabIndex(1)
    );
    let upgraded: serde_json::Value =
        serde_json::from_str(&workspace::save(&restored).unwrap()).unwrap();
    assert_eq!(upgraded["version"], 2);
    let invalid =
        r#"{"version":2,"main":{"kind":"Tabs","tabs":[1,2],"active":1},"windows":[],"focused":99}"#;
    assert!(workspace::restore::<u64>(invalid, viewport()).is_err());
}

#[test]
fn restores_rendered_floating_order_independently_of_focus() {
    use std::collections::HashMap;
    struct Viewer(HashMap<u64, egui::LayerId>);
    impl systemless_ui::egui_dock::TabViewer for Viewer {
        type Tab = u64;
        fn id(&mut self, tab: &mut u64) -> egui::Id {
            egui::Id::new(*tab)
        }
        fn title(&mut self, tab: &mut u64) -> egui::WidgetText {
            format!("Pane {tab}").into()
        }
        fn ui(&mut self, ui: &mut egui::Ui, tab: &mut u64) {
            ui.label(format!("Content {tab}"));
        }
        fn on_tab_button(&mut self, tab: &mut u64, response: &egui::Response) {
            self.0.insert(*tab, response.layer_id);
        }
    }
    let mut state = DockState::new(vec![0u64]);
    for pane in [1, 2] {
        let surface = state.add_window(vec![pane]);
        state
            .get_window_state_mut(surface)
            .unwrap()
            .set_position(egui::pos2(
                100. + pane as f32 * 40.,
                100. + pane as f32 * 30.,
            ))
            .set_size(egui::vec2(300., 240.));
    }
    let focus = state.find_tab(&2).unwrap();
    state.set_focused_node_and_surface(focus.node_path());
    let context = egui::Context::default();
    let theme = Theme::default();
    let mut viewer = Viewer(HashMap::new());
    for frame in 0..3 {
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport()),
                ..Default::default()
            },
            |ui| {
                workspace::show(ui, &theme, &mut state, &mut viewer);
                if frame > 0 {
                    ui.ctx().move_to_top(viewer.0[&1]);
                }
            },
        );
        output.textures_delta.clear();
    }
    let saved =
        workspace::save_with_layers(&state, &context, |pane| viewer.0.get(pane).copied()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(json["windows"][0]["tree"]["tabs"][0], 2);
    assert_eq!(json["windows"][1]["tree"]["tabs"][0], 1);
    assert_eq!(json["windows"][0]["bounds"][0], 180.);
    assert_eq!(json["windows"][1]["bounds"][0], 140.);

    let mut restored: DockState<u64> = workspace::restore(&saved, viewport()).unwrap();
    assert_eq!(*restored.find_active_focused().unwrap().1, 2);
    let restored_context = egui::Context::default();
    let mut restored_viewer = Viewer(HashMap::new());
    for _ in 0..3 {
        let mut output = restored_context.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport()),
                ..Default::default()
            },
            |ui| {
                workspace::show(ui, &theme, &mut restored, &mut restored_viewer);
            },
        );
        output.textures_delta.clear();
    }
    let layers: Vec<_> = restored_context.memory(|memory| memory.layer_ids().collect());
    let rank = |pane| {
        layers
            .iter()
            .position(|layer| *layer == restored_viewer.0[&pane])
            .unwrap()
    };
    assert!(
        rank(1) > rank(2),
        "front pane must survive context recreation and surface renumbering"
    );
    let top = restored_viewer.0[&1];
    let back = restored_viewer.0[&2];
    let overlap = restored_context.memory(|memory| {
        memory
            .area_rect(top.id)
            .unwrap()
            .intersect(memory.area_rect(back.id).unwrap())
    });
    assert!(overlap.is_positive());
    assert_eq!(restored_context.layer_id_at(overlap.center()), Some(top));
    let resaved = workspace::save_with_layers(&restored, &restored_context, |pane| {
        restored_viewer.0.get(pane).copied()
    })
    .unwrap();
    let resaved: serde_json::Value = serde_json::from_str(&resaved).unwrap();
    assert_eq!(
        resaved["windows"], json["windows"],
        "restoration must not drift window positions or inflate their sizes"
    );
    assert_eq!(*restored.find_active_focused().unwrap().1, 2);
}
