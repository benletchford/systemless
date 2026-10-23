//! Docking, split panes and floating surfaces use egui_dock's interaction model.
//! Persistence stores a versioned, validated layout rather than private egui state.
use crate::Theme;
use egui_dock::{DockArea, DockState, Node, NodeIndex, Surface, TabIndex, TabViewer, Tree};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{collections::HashSet, hash::Hash};

pub fn show<T>(
    ui: &mut egui::Ui,
    theme: &Theme,
    state: &mut DockState<T>,
    viewer: &mut impl TabViewer<Tab = T>,
) {
    DockArea::new(state)
        .id(egui::Id::new("systemless-workspace"))
        .style(theme.dock_style(ui.style()))
        .show_inside(ui, viewer);
}
#[derive(Debug)]
pub enum LayoutError {
    Json(serde_json::Error),
    Invalid(&'static str),
}
impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(e) => write!(f, "{e}"),
            Self::Invalid(e) => f.write_str(e),
        }
    }
}
impl std::error::Error for LayoutError {}
impl From<serde_json::Error> for LayoutError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot<T> {
    version: u32,
    main: Option<LayoutNode<T>>,
    windows: Vec<Floating<T>>,
    focused: Option<T>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Floating<T> {
    bounds: [f32; 4],
    tree: LayoutNode<T>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum LayoutNode<T> {
    Tabs {
        tabs: Vec<T>,
        active: usize,
    },
    Split {
        vertical: bool,
        fraction: f32,
        first: Box<Self>,
        second: Box<Self>,
    },
}
fn capture<T: Clone>(tree: &Tree<T>, index: NodeIndex) -> Option<LayoutNode<T>> {
    match &tree[index] {
        Node::Empty => None,
        Node::Leaf(leaf) if leaf.tabs.is_empty() => None,
        Node::Leaf(leaf) => Some(LayoutNode::Tabs {
            tabs: leaf.tabs.clone(),
            active: leaf.active.0,
        }),
        Node::Horizontal(split) | Node::Vertical(split) => Some(LayoutNode::Split {
            vertical: matches!(tree[index], Node::Vertical(_)),
            fraction: split.fraction,
            first: Box::new(capture(tree, index.left())?),
            second: Box::new(capture(tree, index.right())?),
        }),
    }
}
fn seed<T: Clone>(node: &LayoutNode<T>) -> Vec<T> {
    match node {
        LayoutNode::Tabs { tabs, .. } => tabs.clone(),
        LayoutNode::Split { first, .. } => seed(first),
    }
}
fn rebuild<T: Clone>(tree: &mut Tree<T>, index: NodeIndex, node: &LayoutNode<T>) {
    match node {
        LayoutNode::Tabs { tabs, active } => {
            tree[index] = Node::leaf_with(tabs.clone());
            tree[index].get_leaf_mut().unwrap().active = TabIndex(*active);
        }
        LayoutNode::Split {
            vertical,
            fraction,
            first,
            second,
        } => {
            tree[index] = Node::leaf_with(seed(first));
            let [a, b] = if *vertical {
                tree.split_below(index, *fraction, seed(second))
            } else {
                tree.split_right(index, *fraction, seed(second))
            };
            rebuild(tree, a, first);
            rebuild(tree, b, second);
        }
    }
}
fn validate<'a, T: Eq + Hash>(
    node: &'a LayoutNode<T>,
    depth: usize,
    ids: &mut HashSet<&'a T>,
) -> Result<(), LayoutError> {
    // egui_dock uses a heap-indexed tree: cap depth before allocating it.
    if depth > 12 {
        return Err(LayoutError::Invalid("Layout exceeds maximum nesting depth"));
    }
    match node {
        LayoutNode::Tabs { tabs, active } => {
            if tabs.is_empty() || *active >= tabs.len() {
                return Err(LayoutError::Invalid("Invalid active tab"));
            }
            for tab in tabs {
                if !ids.insert(tab) {
                    return Err(LayoutError::Invalid("Duplicate pane identity"));
                }
            }
            if ids.len() > 512 {
                return Err(LayoutError::Invalid("Too many panes"));
            }
        }
        LayoutNode::Split {
            fraction,
            first,
            second,
            ..
        } => {
            if !fraction.is_finite() || !(0.01..=0.99).contains(fraction) {
                return Err(LayoutError::Invalid("Invalid split ratio"));
            }
            validate(first, depth + 1, ids)?;
            validate(second, depth + 1, ids)?;
        }
    }
    Ok(())
}
/// Save tab order, active tabs, focused pane, split ratios and floating trees.
/// For rendered floating geometry and stacking, use `save_with_layers`.
/// Pane data should contain stable application IDs, not heavyweight application state.
pub fn save<T: Clone + Serialize>(state: &DockState<T>) -> Result<String, LayoutError> {
    save_ordered(state, |_| 0, |_| None)
}
/// Save rendered floating geometry and back-to-front order. Record each tab's
/// `Response::layer_id` in `TabViewer::on_tab_button` and supply it here. This
/// keeps stacking independent of keyboard focus, without depending on dock
/// window IDs. Unobserved windows retain their relative surface order.
pub fn save_with_layers<T: Clone + Serialize>(
    state: &DockState<T>,
    context: &egui::Context,
    layer_for: impl Fn(&T) -> Option<egui::LayerId>,
) -> Result<String, LayoutError> {
    let layers: Vec<_> = context.memory(|memory| memory.layer_ids().collect());
    save_ordered(
        state,
        |tree| {
            tree.tabs()
                .filter_map(&layer_for)
                .find_map(|layer| layers.iter().position(|candidate| *candidate == layer))
                .map_or(0, |rank| rank + 1)
        },
        |tree| {
            let layer = tree.tabs().find_map(&layer_for)?;
            context.memory(|memory| memory.area_rect(layer.id))
        },
    )
}
fn save_ordered<T: Clone + Serialize>(
    state: &DockState<T>,
    rank: impl Fn(&Tree<T>) -> usize,
    geometry: impl Fn(&Tree<T>) -> Option<egui::Rect>,
) -> Result<String, LayoutError> {
    let mut windows = vec![];
    for surface in state.iter_surfaces() {
        if let Surface::Window(tree, window) = surface {
            if let Some(node) = capture(tree, NodeIndex::root()) {
                let r = geometry(tree).unwrap_or_else(|| window.rect());
                let bounds = if r.is_finite() && r.is_positive() {
                    [r.min.x, r.min.y, r.width(), r.height()]
                } else {
                    [80., 80., 480., 320.]
                };
                windows.push((rank(tree), Floating { bounds, tree: node }));
            }
        }
    }
    windows.sort_by_key(|(rank, _)| *rank);
    let windows = windows.into_iter().map(|(_, window)| window).collect();
    let main = if state.main_surface().is_empty() {
        None
    } else {
        capture(state.main_surface(), NodeIndex::root())
    };
    Ok(serde_json::to_string_pretty(&Snapshot {
        version: 2,
        main,
        windows,
        focused: state
            .focused_leaf()
            .and_then(|path| state[path].get_leaf())
            .and_then(|leaf| leaf.tabs.get(leaf.active.0))
            .cloned(),
    })?)
}
/// Restore into a new state. On error the caller's live workspace remains untouched.
/// Geometry is constrained to the host's visible rectangle, recovering offscreen windows.
/// Callers must also validate saved pane IDs against their application catalogue.
pub fn restore<T: Clone + Eq + Hash + DeserializeOwned>(
    text: &str,
    viewport: egui::Rect,
) -> Result<DockState<T>, LayoutError> {
    if text.len() > 1_048_576 {
        return Err(LayoutError::Invalid("Layout file too large"));
    }
    if !viewport.is_finite() || !viewport.is_positive() {
        return Err(LayoutError::Invalid("Invalid workspace bounds"));
    }
    let snapshot: Snapshot<T> = serde_json::from_str(text)?;
    if !matches!(snapshot.version, 1 | 2) {
        return Err(LayoutError::Invalid("Unsupported layout version"));
    }
    if snapshot.windows.len() > 64 {
        return Err(LayoutError::Invalid("Too many windows"));
    }
    let mut ids = HashSet::new();
    if let Some(main) = &snapshot.main {
        validate(main, 0, &mut ids)?;
    }
    for window in &snapshot.windows {
        validate(&window.tree, 0, &mut ids)?;
        if window.bounds.iter().any(|v| !v.is_finite())
            || window.bounds[2] <= 0.
            || window.bounds[3] <= 0.
        {
            return Err(LayoutError::Invalid("Invalid window bounds"));
        }
    }
    if snapshot
        .focused
        .as_ref()
        .is_some_and(|focused| !ids.iter().any(|id| *id == focused))
    {
        return Err(LayoutError::Invalid("Focused pane is missing"));
    }
    let mut state = DockState::new(vec![]);
    if let Some(main) = &snapshot.main {
        *state.main_surface_mut() = Tree::new(vec![]);
        rebuild(state.main_surface_mut(), NodeIndex::root(), main);
    }
    for window in snapshot.windows {
        let index = state.add_window(vec![]);
        rebuild(&mut state[index], NodeIndex::root(), &window.tree);
        let [x, y, w, h] = window.bounds;
        let size = egui::vec2(w.min(viewport.width()), h.min(viewport.height()));
        let position = egui::pos2(
            x.clamp(viewport.left(), viewport.right() - size.x),
            y.clamp(viewport.top(), viewport.bottom() - size.y),
        );
        state
            .get_window_state_mut(index)
            .unwrap()
            .set_position(position)
            .set_size(size);
    }
    if let Some(focused) = snapshot.focused {
        let path = state
            .find_tab(&focused)
            .ok_or(LayoutError::Invalid("Focused pane is missing"))?;
        state
            .set_active_tab(path)
            .map_err(|_| LayoutError::Invalid("Invalid focused pane"))?;
        state.set_focused_node_and_surface(path.node_path());
    }
    Ok(state)
}
