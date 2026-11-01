use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::base::motion::{Transition, transition};
use gpui_kit::*;

const POSITION_TRANSITION: Duration = Duration::from_millis(150);

/// Glides a child to its laid out position, relative to the origin of a viewport.
pub struct MotionSlot {
    id: ElementId,
    viewport_origin: Rc<Cell<Point<Pixels>>>,
    child: AnyElement,
}

impl MotionSlot {
    pub fn new(
        id: impl Into<ElementId>,
        viewport_origin: Rc<Cell<Point<Pixels>>>,
        child: impl IntoElement,
    ) -> Self {
        Self {
            id: id.into(),
            viewport_origin,
            child: child.into_any_element(),
        }
    }
}

impl IntoElement for MotionSlot {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for MotionSlot {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        // Relative to the viewport so scrolling animates
        let target = bounds.origin - self.viewport_origin.get();
        let shown = transition(
            "position",
            target,
            Transition::new(POSITION_TRANSITION),
            window,
            cx,
        );
        window.with_element_offset(shown - target, |window| {
            self.child.prepaint(window, cx);
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}
