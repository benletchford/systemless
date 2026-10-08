//! Accessibility state missing from GPUI's standard element setters.
use gpui_kit::{
    accesskit, A11ySubtreeBuilder, App, Bounds, Element, ElementId,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, Role, Window,
};

pub struct AccessibleState<E> {
    inner: E,
    disabled: bool,
}

impl<E: Element> AccessibleState<E> {
    pub fn new(inner: E, disabled: bool) -> Self {
        Self { inner, disabled }
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
    fn a11y_role(&self) -> Option<Role> { self.inner.a11y_role() }
    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        self.inner.write_a11y_info(node);
        if self.disabled { node.set_disabled(); }
    }
    fn a11y_synthetic_children(
        &mut self, prepaint: &mut Self::PrepaintState, builder: &mut A11ySubtreeBuilder,
    ) {
        self.inner.a11y_synthetic_children(prepaint, builder);
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
        self.inner.prepaint(id, inspector, bounds, layout, window, cx)
    }
    fn paint(
        &mut self, id: Option<&GlobalElementId>, inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>, layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState, window: &mut Window, cx: &mut App,
    ) {
        self.inner.paint(id, inspector, bounds, layout, prepaint, window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::AccessibleState;
    use gpui_kit::{accesskit, div, Element, InteractiveElement, Role, StatefulInteractiveElement, Toggled};

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
}
