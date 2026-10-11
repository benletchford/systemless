//! Accessibility state missing from GPUI's standard element setters.
use gpui_kit::{
    accesskit, A11ySubtreeBuilder, App, Bounds, Element, ElementId,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, Role, Window,
};

/// Preserve a Kit component's native role and actions while supplying the
/// disabled state missing from its rendered root.
#[derive(gpui_kit::IntoElement)]
pub struct AccessibleComponent<C: gpui_kit::RenderOnce + 'static> {
    inner: C,
    disabled: bool,
}

impl<C: gpui_kit::RenderOnce + 'static> AccessibleComponent<C> {
    pub fn new(inner: C, disabled: bool) -> Self {
        Self { inner, disabled }
    }
}

impl<C: gpui_kit::RenderOnce + 'static> gpui_kit::RenderOnce for AccessibleComponent<C> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        AccessibleState::new(self.inner.render(window, cx).into_element(), self.disabled)
    }
}

pub struct AccessibleState<E> {
    inner: E,
    disabled: bool,
    hidden: bool,
    text: Option<(String, bool)>,
    selection: Option<std::ops::Range<usize>>,
    lines: Option<Vec<(usize, String)>>,
    line_ids: Option<std::rc::Rc<std::cell::RefCell<Vec<(usize, usize, accesskit::NodeId)>>>>,
    run_id: Option<std::rc::Rc<std::cell::Cell<Option<accesskit::NodeId>>>>,
    positions: Option<Vec<f32>>,
    painted_positions: Option<Box<dyn Fn(Bounds<Pixels>) -> Option<Vec<f32>>>>,
}

impl<E: Element> AccessibleState<E> {
    pub fn new(inner: E, disabled: bool) -> Self {
        Self { inner, disabled, hidden: false, text: None, selection: None, lines: None, line_ids: None, run_id: None, positions: None, painted_positions: None }
    }

    /// Keep the element painted while excluding its modal-background subtree
    /// from assistive navigation. Keyboard focus must be disabled separately.
    pub fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

    /// Guest Mac Roman bytes decode one-to-one into BMP scalar positions.
    /// Geometry remains optional until exact per-character guest bounds exist.
    pub fn single_line_selection(mut self, range: std::ops::Range<usize>) -> Self {
        self.selection = Some(range);
        self
    }

    pub fn guest_text_lines(mut self, bytes: &[u8], starts: &[usize], selection: std::ops::Range<usize>) -> Self {
        if starts.first() == Some(&0) && starts.last() == Some(&bytes.len())
            && starts.windows(2).all(|pair| pair[0] <= pair[1] && pair[1] <= bytes.len())
            && selection.start <= selection.end && selection.end <= bytes.len() {
            self.lines = Some(starts.windows(2).map(|pair| (pair[0],
                systemless::systems::macintosh::mac_roman::decode_mac_roman(&bytes[pair[0]..pair[1]]).replace('\r', "\n"))).collect());
            if self.lines.as_ref().is_some_and(Vec::is_empty) {
                self.lines = Some(vec![(0, String::new())]);
            }
            if let Some(lines) = self.lines.as_mut() {
                if lines.last().is_some_and(|(_, value)| value.ends_with('\n')) {
                    lines.push((bytes.len(), String::new()));
                }
            }
            self.selection = Some(selection);
        }
        self
    }

    pub fn guest_line_ids(mut self, ids: std::rc::Rc<std::cell::RefCell<Vec<(usize, usize, accesskit::NodeId)>>>) -> Self {
        self.line_ids = Some(ids);
        self
    }

    pub fn single_line_run_id(mut self, id: std::rc::Rc<std::cell::Cell<Option<accesskit::NodeId>>>) -> Self {
        self.run_id = Some(id);
        self
    }

    pub fn single_line_positions(mut self, positions: Vec<f32>) -> Self {
        self.positions = Some(positions);
        self
    }

    /// Read geometry only after the wrapped painter has refreshed its layout.
    pub fn single_line_painted_positions(mut self, read: impl Fn(Bounds<Pixels>) -> Option<Vec<f32>> + 'static) -> Self {
        self.painted_positions = Some(Box::new(read));
        self
    }

    /// Supply text-field semantics without reshaping glyphs or claiming
    /// unsupported accessibility editing actions. Guest bytes remain owned
    /// by the existing Toolbox/event path.
    pub fn text_value(mut self, value: String, multiline: bool) -> Self {
        self.text = Some((value.replace('\r', "\n"), multiline));
        self
    }
}

impl<E: Element + gpui_kit::InteractiveElement> gpui_kit::InteractiveElement for AccessibleState<E> {
    fn interactivity(&mut self) -> &mut gpui_kit::Interactivity {
        self.inner.interactivity()
    }
}

impl<E: Element> IntoElement for AccessibleState<E> {
    type Element = Self;
    fn into_element(self) -> Self { self }
}

impl<E: Element> Element for AccessibleState<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;

    fn id(&self) -> Option<ElementId> { self.inner.id() }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.inner.source_location()
    }
    fn a11y_role(&self) -> Option<Role> {
        self.text.as_ref().map(|(_, multiline)| if *multiline {
            Role::MultilineTextInput
        } else { Role::TextInput }).or_else(|| self.inner.a11y_role())
    }
    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        self.inner.write_a11y_info(node);
        if let Some((value, _)) = &self.text { node.set_value(value.clone()); }
        if self.disabled { node.set_disabled(); }
        if self.hidden { node.set_hidden(); }
    }
    fn a11y_synthetic_children(
        &mut self, prepaint: &mut Self::PrepaintState, builder: &mut A11ySubtreeBuilder,
    ) {
        self.inner.a11y_synthetic_children(prepaint, builder);
        if let Some(id) = &self.run_id { id.set(None); }
        if let Some(ids) = &self.line_ids { ids.borrow_mut().clear(); }
        if self.hidden { return; }
        if let (Some(lines), Some(range)) = (&self.lines, &self.selection) {
            let mut positions = Vec::new();
            for (index, (start, value)) in lines.iter().enumerate() {
                let id = builder.synthetic_node_id(("guest-text-line", index));
                let mut node = accesskit::Node::new(Role::TextRun);
                node.set_value(value.clone());
                node.set_character_lengths(value.chars().map(|ch| ch.len_utf8() as u8).collect::<Vec<_>>());
                node.set_text_direction(accesskit::TextDirection::LeftToRight);
                if builder.push_child(id, node) { positions.push((*start, value.chars().count(), id)); }
            }
            if let Some(ids) = &self.line_ids { *ids.borrow_mut() = positions.clone(); }
            if let Some(selection) = guest_multiline_selection(&positions, range.clone()) {
                builder.parent_node().set_text_selection(selection);
            }
            return;
        }

        if let (Some((value, false)), Some(range)) = (&self.text, &self.selection) {
            let id = builder.synthetic_node_id("guest-single-line-text");
            if let Some((mut run, selection)) = guest_single_line_run(value, range.clone(), id) {
                if let Some(bounds) = builder.parent_node().bounds() { run.set_bounds(bounds); }
                if let Some(positions) = &self.positions {
                    apply_guest_character_positions(&mut run, positions);
                }

                if builder.push_child(id, run) {
                    builder.parent_node().set_text_selection(selection);
                    if let Some(shared) = &self.run_id { shared.set(Some(id)); }
                }
            }
        }

    }
    fn request_layout(
        &mut self, id: Option<&GlobalElementId>, inspector: Option<&InspectorElementId>,
        window: &mut Window, cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.inner.request_layout(id, inspector, window, cx)
    }
    fn prepaint(
        &mut self, id: Option<&GlobalElementId>, inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>, layout: &mut Self::RequestLayoutState,
        window: &mut Window, cx: &mut App,
    ) -> Self::PrepaintState {
        let state = self.inner.prepaint(id, inspector, bounds, layout, window, cx);
        if let Some(read) = &self.painted_positions { self.positions = read(bounds); }
        state
    }
    fn paint(
        &mut self, id: Option<&GlobalElementId>, inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>, layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState, window: &mut Window, cx: &mut App,
    ) {
        self.inner.paint(id, inspector, bounds, layout, prepaint, window, cx);
    }
}

pub(crate) fn guest_multiline_range(selection: &accesskit::TextSelection,
    lines: &[(usize, usize, accesskit::NodeId)], length: usize) -> Option<std::ops::Range<usize>> {
    let offset = |position: accesskit::TextPosition| {
        let (start, len, _) = lines.iter().find(|(_, _, node)| *node == position.node)?;
        if position.character_index > *len { return None; }
        start.checked_add(position.character_index).filter(|offset| *offset <= length)
    };
    let anchor = offset(selection.anchor)?; let focus = offset(selection.focus)?;
    Some(anchor.min(focus)..anchor.max(focus))
}

fn guest_multiline_selection(lines: &[(usize, usize, accesskit::NodeId)], range: std::ops::Range<usize>) -> Option<accesskit::TextSelection> {
    let position = |offset: usize| lines.iter().rev().find(|(start, len, _)|
        offset >= *start && offset <= start.checked_add(*len).unwrap_or(0)).map(|(start, _, node)|
            accesskit::TextPosition { node: *node, character_index: offset - start });
    Some(accesskit::TextSelection { anchor: position(range.start)?, focus: position(range.end)? })
}

pub(crate) fn guest_selection_range(selection: &accesskit::TextSelection, id: Option<accesskit::NodeId>, length: usize)
    -> Option<std::ops::Range<usize>> {
    let id = id?;
    if selection.anchor.node != id || selection.focus.node != id { return None; }
    let start = selection.anchor.character_index.min(selection.focus.character_index);
    let end = selection.anchor.character_index.max(selection.focus.character_index);
    (end <= length).then_some(start..end)
}

fn apply_guest_character_positions(node: &mut accesskit::Node, positions: &[f32]) -> bool {
    if positions.len() != node.character_lengths().len() + 1
        || positions.iter().any(|x| !x.is_finite())
        || positions.windows(2).any(|pair| pair[1] < pair[0]) { return false; }
    node.set_character_positions(positions[..positions.len() - 1].to_vec());
    node.set_character_widths(positions.windows(2).map(|pair| pair[1] - pair[0]).collect::<Vec<_>>());
    true
}

fn guest_single_line_run(value: &str, range: std::ops::Range<usize>, id: accesskit::NodeId)
    -> Option<(accesskit::Node, accesskit::TextSelection)> {
    if value.contains(['\r', '\n']) || range.start > range.end || range.end > value.chars().count() { return None; }
    let mut node = accesskit::Node::new(Role::TextRun);
    node.set_value(value.to_owned());
    node.set_character_lengths(value.chars().map(|ch| ch.len_utf8() as u8).collect::<Vec<_>>());
    node.set_text_direction(accesskit::TextDirection::LeftToRight);
    Some((node, accesskit::TextSelection {
        anchor: accesskit::TextPosition { node: id, character_index: range.start },
        focus: accesskit::TextPosition { node: id, character_index: range.end },
    }))
}

#[cfg(test)]
mod tests {
    use super::AccessibleState;
    use gpui_kit::{accesskit, div, Element, InteractiveElement, Role, StatefulInteractiveElement, Toggled};

    #[test]
    fn multiline_guest_offsets_keep_wrap_and_break_boundaries() {
        let empty = AccessibleState::new(div().id("empty"), false).guest_text_lines(&[], &[0], 0..0);
        assert_eq!(empty.lines.unwrap(), vec![(0, String::new())]);
        let terminated = AccessibleState::new(div().id("terminated"), false).guest_text_lines(b"A\r", &[0, 2], 2..2);
        assert_eq!(terminated.lines.unwrap(), vec![(0, "A\n".into()), (2, String::new())]);

        let lines = [(0, 3, accesskit::NodeId(1)), (3, 2, accesskit::NodeId(2))];
        let selection = super::guest_multiline_selection(&lines, 2..4).unwrap();
        assert_eq!(selection.anchor.node, accesskit::NodeId(1));
        assert_eq!(selection.anchor.character_index, 2);
        assert_eq!(selection.focus.node, accesskit::NodeId(2));
        assert_eq!(selection.focus.character_index, 1);
        assert_eq!(super::guest_multiline_range(&selection, &lines, 5), Some(2..4));
        assert!(super::guest_multiline_range(&selection, &lines[..1], 5).is_none());
        assert!(super::guest_multiline_range(&selection, &lines, 3).is_none());

        assert_eq!(super::guest_multiline_selection(&lines, 3..3).unwrap().focus.node, accesskit::NodeId(2));
        assert!(super::guest_multiline_selection(&lines, 0..6).is_none());
    }

    #[test]
    fn single_line_guest_selection_keeps_roman_character_boundaries() {
        let (node, selection) = super::guest_single_line_run("Café", 3..4, accesskit::NodeId(1)).unwrap();
        assert_eq!(node.value(), Some("Café"));
        assert_eq!(node.character_lengths(), &[1, 1, 1, 2]);
        assert_eq!(selection.anchor.character_index, 3);
        assert_eq!(selection.focus.character_index, 4);
        assert_eq!(super::guest_selection_range(&selection, Some(accesskit::NodeId(1)), 4), Some(3..4));
        assert!(super::guest_selection_range(&selection, Some(accesskit::NodeId(2)), 4).is_none());
        assert!(super::guest_selection_range(&selection, None, 4).is_none());
        assert!(super::guest_selection_range(&selection, Some(accesskit::NodeId(1)), 3).is_none());

        let mut node = node;
        assert!(super::apply_guest_character_positions(&mut node, &[-3., 5., 12., 18., 24.]));
        assert_eq!(node.character_positions().unwrap(), &[-3., 5., 12., 18.]);
        assert_eq!(node.character_widths().unwrap(), &[8., 7., 6., 6.]);
        assert!(!super::apply_guest_character_positions(&mut node, &[0., f32::NAN]));
        assert!(!super::apply_guest_character_positions(&mut node, &[0., 1., 2., 1., 3.]));

        assert!(super::guest_single_line_run("Café", 3..5, accesskit::NodeId(1)).is_none());
        assert!(super::guest_single_line_run("one\nTwo", 0..0, accesskit::NodeId(1)).is_none());
    }

    #[test]
    fn disabled_state_preserves_menu_role_label_and_checkmark() {
        for disabled in [false, true] {
            let row = div().id("item").role(Role::MenuItemCheckBox)
                .aria_label("Sound").aria_toggled(Toggled::True);
            let wrapped = AccessibleState::new(row, disabled);
            assert_eq!(Element::id(&wrapped), Some("item".into()));
            let mut node = accesskit::Node::new(wrapped.a11y_role().unwrap());
            wrapped.write_a11y_info(&mut node);
            assert_eq!(node.role(), Role::MenuItemCheckBox);
            assert_eq!(node.label(), Some("Sound"));
            assert_eq!(node.toggled(), Some(Toggled::True));
            assert_eq!(node.is_disabled(), disabled);
        }
    }
    #[test]
    fn hidden_modal_background_preserves_node_identity_and_label() {
        for hidden in [false, true, false] {
            let row = div().id("save-panel").role(Role::Group).aria_label("Save file");
            let wrapped = AccessibleState::new(row, false).hidden(hidden);
            let mut node = accesskit::Node::new(wrapped.a11y_role().unwrap());
            wrapped.write_a11y_info(&mut node);
            assert_eq!(Element::id(&wrapped), Some("save-panel".into()));
            assert_eq!(node.role(), Role::Group);
            assert_eq!(node.label(), Some("Save file"));
            assert_eq!(node.is_hidden(), hidden);
            assert!(!node.is_disabled());
        }
    }

    #[test]
    fn guest_text_value_preserves_identity_state_and_roman_content() {
        let guest = b"Caf\x8e\rSecond";
        let value = systemless::systems::macintosh::mac_roman::decode_mac_roman(guest);
        for multiline in [false, true] {
            let wrapped = AccessibleState::new(div().id("guest-field"), true)
                .hidden(true).text_value(value.clone(), multiline);
            let mut node = accesskit::Node::new(wrapped.a11y_role().unwrap());
            wrapped.write_a11y_info(&mut node);
            assert_eq!(Element::id(&wrapped), Some("guest-field".into()));
            assert_eq!(node.role(), if multiline { Role::MultilineTextInput } else { Role::TextInput });
            assert_eq!(node.value(), Some("Café\nSecond"));
            assert!(node.is_disabled() && node.is_hidden());
            assert!(!node.supports_action(accesskit::Action::SetValue));
            assert!(!node.supports_action(accesskit::Action::SetTextSelection));
        }
    }

}
