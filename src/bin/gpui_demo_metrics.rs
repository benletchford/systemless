//! Relative component metrics for the guest scene, independent of host chrome.
use gpui_kit::*;

pub struct SceneMetrics<E> {
    inner: E,
    rem_size: Pixels,
}

#[cfg(test)]
mod tests {
    use super::SceneMetrics;
    use gpui_kit::{div, px, Element, Role, accesskit};
    use gpui_kit::prelude::*;

    #[test]
    fn scene_metrics_preserves_modal_background_accessibility() {
        let element = super::super::a11y::AccessibleState::new(
            div().id("modal-background").role(Role::Group).aria_label("Guest scene"), false).hidden(true);
        let wrapped = SceneMetrics::new(element, px(32.));
        assert_eq!(wrapped.a11y_role(), Some(Role::Group));
        assert_eq!(wrapped.id(), Some("modal-background".into()));
        let mut node = accesskit::Node::new(Role::Group);
        wrapped.write_a11y_info(&mut node);
        assert!(node.is_hidden());
        assert_eq!(node.label(), Some("Guest scene"));
    }
}

impl<E: Element> SceneMetrics<E> {
    pub fn new(inner: E, rem_size: Pixels) -> Self {
        Self { inner, rem_size }
    }
}

impl<E: Element> IntoElement for SceneMetrics<E> {
    type Element = Self;
    fn into_element(self) -> Self { self }
}

impl<E: Element> Element for SceneMetrics<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;

    fn id(&self) -> Option<ElementId> { self.inner.id() }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.inner.source_location()
    }
    fn a11y_role(&self) -> Option<Role> { self.inner.a11y_role() }
    fn write_a11y_info(&self, node: &mut accesskit::Node) { self.inner.write_a11y_info(node); }
    fn a11y_synthetic_children(&mut self, state: &mut Self::PrepaintState, builder: &mut A11ySubtreeBuilder) {
        self.inner.a11y_synthetic_children(state, builder);
    }
    fn request_layout(
        &mut self, id: Option<&GlobalElementId>, inspector: Option<&InspectorElementId>,
        window: &mut Window, cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        window.with_rem_size(Some(self.rem_size), |window| {
            self.inner.request_layout(id, inspector, window, cx)
        })
    }
    fn prepaint(
        &mut self, id: Option<&GlobalElementId>, inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>, layout: &mut Self::RequestLayoutState,
        window: &mut Window, cx: &mut App,
    ) -> Self::PrepaintState {
        window.with_rem_size(Some(self.rem_size), |window| {
            self.inner.prepaint(id, inspector, bounds, layout, window, cx)
        })
    }
    fn paint(
        &mut self, id: Option<&GlobalElementId>, inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>, layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState, window: &mut Window, cx: &mut App,
    ) {
        window.with_rem_size(Some(self.rem_size), |window| {
            self.inner.paint(id, inspector, bounds, layout, prepaint, window, cx)
        });
    }
}
