//! Relative component metrics for the guest scene, independent of host chrome.
use gpui_kit::*;

pub struct SceneMetrics<E> {
    inner: E,
    rem_size: Pixels,
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
