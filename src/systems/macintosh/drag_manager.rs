//! Process-owned classic Drag Manager records. Guest memory adapters must copy
//! flavor bytes into these records; guest callbacks are invoked outside borrows.
//! Contracts and errors: Apple Drag Manager Reference, Drag.h.

use std::collections::BTreeMap;

pub(crate) const BAD_DRAG_REF_ERR: i16 = -1850;
pub(crate) const BAD_DRAG_ITEM_ERR: i16 = -1851;
pub(crate) const BAD_DRAG_FLAVOR_ERR: i16 = -1852;
pub(crate) const DUPLICATE_FLAVOR_ERR: i16 = -1853;
pub(crate) const CANT_GET_FLAVOR_ERR: i16 = -1854;
pub(crate) const DUPLICATE_HANDLER_ERR: i16 = -1855;
pub(crate) const HANDLER_NOT_FOUND_ERR: i16 = -1856;

/// A ZoomRects path in global QuickDraw coordinates. The exact rounding and
/// acceleration curves still need a classic-system comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ZoomRectPath {
    from: [i16; 4],
    to: [i16; 4],
    steps: u16,
    acceleration: i16,
}

impl ZoomRectPath {
    pub(crate) fn new(from: [i16; 4], to: [i16; 4], steps: i16, acceleration: i16) -> Option<Self> {
        if !(0..=2).contains(&acceleration) {
            return None;
        }
        Some(Self {
            from,
            to,
            steps: steps.clamp(4, 25) as u16,
            acceleration,
        })
    }

    pub(crate) fn steps(self) -> u16 {
        self.steps
    }

    pub(crate) fn frame(self, step: u16) -> [i16; 4] {
        let count = i64::from(self.steps);
        let position = i64::from(step.min(self.steps));
        let (numerator, denominator) = match self.acceleration {
            1 => (position * position, count * count),
            2 => (
                count * count - (count - position) * (count - position),
                count * count,
            ),
            _ => (position, count),
        };
        std::array::from_fn(|index| {
            let start = i64::from(self.from[index]);
            let delta = i64::from(self.to[index]) - start;
            (start + delta * numerator / denominator) as i16
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MouseMoveWaitResult {
    Moved,
    Released,
    Waiting,
}

pub(crate) fn wait_mouse_moved(
    initial: (i16, i16),
    current: (i16, i16),
    held: bool,
) -> MouseMoveWaitResult {
    if !held {
        MouseMoveWaitResult::Released
    } else if current != initial {
        MouseMoveWaitResult::Moved
    } else {
        MouseMoveWaitResult::Waiting
    }
}

/// Two-pixel drag highlight, excluding the frame boundary (Apple ShowDragHilite).
pub(crate) fn hilite_pixel(
    point: (i16, i16),
    inside: bool,
    contains: impl Fn(i16, i16) -> bool,
) -> bool {
    if contains(point.0, point.1) != inside {
        return false;
    }
    for radius in 1i16..=3 {
        for dv in -radius..=radius {
            for dh in -radius..=radius {
                if dv.abs().max(dh.abs()) != radius {
                    continue;
                }
                let neighbor = point.0.checked_add(dv).zip(point.1.checked_add(dh));
                let covered = neighbor.is_some_and(|(v, h)| contains(v, h));
                if covered != inside {
                    return radius >= 2;
                }
            }
        }
    }
    false
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DragHandlerKind {
    Tracking,
    Receive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DragHandler {
    pub(crate) kind: DragHandlerKind,
    pub(crate) window: u32,
    pub(crate) procedure: u32,
    pub(crate) refcon: u32,
    pub(crate) rtoc: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DragTrackingMessage {
    pub(crate) handler: DragHandler,
    pub(crate) window: u32,
    pub(crate) message: u16,
}

/// Apple Drag.h: enter handler=1, enter window=2, in window=3,
/// leave window=4, leave handler=5. Global handlers stay entered across windows.
pub(crate) fn tracking_messages(
    previous_window: Option<u32>,
    previous: &[DragHandler],
    next_window: Option<u32>,
    next: &[DragHandler],
) -> Vec<DragTrackingMessage> {
    let mut messages = Vec::new();
    let changed = previous_window != next_window;
    if let Some(window) = previous_window {
        for handler in previous {
            if changed || !next.contains(handler) {
                messages.push(DragTrackingMessage {
                    handler: *handler,
                    window,
                    message: 4,
                });
            }
        }
        for handler in previous {
            if !next.contains(handler) {
                messages.push(DragTrackingMessage {
                    handler: *handler,
                    window,
                    message: 5,
                });
            }
        }
    }
    if let Some(window) = next_window {
        for handler in next {
            if !previous.contains(handler) {
                messages.push(DragTrackingMessage {
                    handler: *handler,
                    window,
                    message: 1,
                });
            }
        }
        for handler in next {
            if changed || !previous.contains(handler) {
                messages.push(DragTrackingMessage {
                    handler: *handler,
                    window,
                    message: 2,
                });
            }
        }
        for handler in next {
            messages.push(DragTrackingMessage {
                handler: *handler,
                window,
                message: 3,
            });
        }
    }
    messages
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DragFlavor {
    pub(crate) flavor_type: u32,
    pub(crate) flags: u32,
    /// None represents promised data, not an empty flavor.
    pub(crate) data: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DragItem {
    pub(crate) reference: u32,
    pub(crate) bounds: Option<(i16, i16, i16, i16)>,
    pub(crate) flavors: Vec<DragFlavor>,
}

/// Owned RGB snapshot of a drag image; guest handles may be disposed after setup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DragImage {
    pub(crate) offset: (i16, i16),
    pub(crate) flags: u32,
    pub(crate) pixels: Vec<(i16, i16, [u16; 3])>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DragRecord {
    pub(crate) image: Option<DragImage>,
    pub(crate) items: Vec<DragItem>,
    pub(crate) attributes: u32,
    pub(crate) origin: (i16, i16),
    pub(crate) mouse: (i16, i16),
}

impl DragRecord {
    pub(crate) fn add_flavor(&mut self, item: u32, flavor: DragFlavor) -> Result<(), i16> {
        let index = self.items.iter().position(|entry| entry.reference == item);
        if let Some(index) = index {
            if self.items[index]
                .flavors
                .iter()
                .any(|entry| entry.flavor_type == flavor.flavor_type)
            {
                return Err(DUPLICATE_FLAVOR_ERR);
            }
            self.items[index].flavors.push(flavor);
        } else {
            self.items.push(DragItem {
                reference: item,
                flavors: vec![flavor],
                ..DragItem::default()
            });
        }
        Ok(())
    }

    pub(crate) fn item(&self, reference: u32) -> Result<&DragItem, i16> {
        self.items
            .iter()
            .find(|item| item.reference == reference)
            .ok_or(BAD_DRAG_ITEM_ERR)
    }

    pub(crate) fn item_bounds(&self, reference: u32) -> Result<(i16, i16, i16, i16), i16> {
        let item = self.item(reference)?;
        let Some((top, left, bottom, right)) = item.bounds else {
            return Ok((self.mouse.0, self.mouse.1, self.mouse.0, self.mouse.1));
        };
        let dv = self.mouse.0.wrapping_sub(self.origin.0);
        let dh = self.mouse.1.wrapping_sub(self.origin.1);
        Ok((
            top.wrapping_add(dv),
            left.wrapping_add(dh),
            bottom.wrapping_add(dv),
            right.wrapping_add(dh),
        ))
    }

    pub(crate) fn item_mut(&mut self, reference: u32) -> Result<&mut DragItem, i16> {
        self.items
            .iter_mut()
            .find(|item| item.reference == reference)
            .ok_or(BAD_DRAG_ITEM_ERR)
    }

    pub(crate) fn flavor(&self, item: u32, flavor_type: u32) -> Result<&DragFlavor, i16> {
        self.item(item)?
            .flavors
            .iter()
            .find(|flavor| flavor.flavor_type == flavor_type)
            .ok_or(BAD_DRAG_FLAVOR_ERR)
    }

    pub(crate) fn flavor_data(
        &self,
        item: u32,
        flavor_type: u32,
        offset: usize,
        capacity: usize,
    ) -> Result<&[u8], i16> {
        let data = self
            .flavor(item, flavor_type)?
            .data
            .as_deref()
            .ok_or(CANT_GET_FLAVOR_ERR)?;
        if offset > data.len() {
            return Err(-50);
        }
        let count = capacity.min(data.len() - offset);
        Ok(&data[offset..offset + count])
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct DragManager {
    next_reference: u32,
    records: BTreeMap<u32, DragRecord>,
    handlers: Vec<DragHandler>,
}

impl DragManager {
    pub(crate) fn is_pristine(&self) -> bool {
        self.next_reference == 0 && self.records.is_empty() && self.handlers.is_empty()
    }

    pub(crate) fn install_handler(&mut self, handler: DragHandler) -> Result<(), i16> {
        if handler.procedure == 0 {
            return Err(-50);
        }
        if self.handlers.iter().any(|entry| {
            entry.kind == handler.kind
                && entry.window == handler.window
                && entry.procedure == handler.procedure
        }) {
            return Err(DUPLICATE_HANDLER_ERR);
        }
        // Multiple distinct handlers can be registered on one window.
        // A NIL window registers an application-wide handler.
        self.handlers.push(handler);
        Ok(())
    }

    pub(crate) fn remove_handler(
        &mut self,
        kind: DragHandlerKind,
        window: u32,
        procedure: u32,
    ) -> Result<(), i16> {
        let index = self
            .handlers
            .iter()
            .position(|entry| {
                entry.kind == kind && entry.window == window && entry.procedure == procedure
            })
            .ok_or(HANDLER_NOT_FOUND_ERR)?;
        self.handlers.remove(index);
        Ok(())
    }

    pub(crate) fn handlers_for_window(
        &self,
        kind: DragHandlerKind,
        window: u32,
    ) -> Vec<DragHandler> {
        // Return owned callback descriptions so invoking guest code never
        // retains a manager borrow across a reentrant handler call.
        self.handlers
            .iter()
            .copied()
            .filter(|entry| entry.kind == kind && (entry.window == 0 || entry.window == window))
            .collect()
    }

    pub(crate) fn remove_window(&mut self, window: u32) {
        if window != 0 {
            self.handlers.retain(|entry| entry.window != window);
        }
    }

    pub(crate) fn new_drag(&mut self) -> Result<u32, i16> {
        let reference = self.next_reference.checked_add(1).ok_or(-108i16)?;
        self.next_reference = reference;
        self.records.insert(reference, DragRecord::default());
        Ok(reference)
    }

    pub(crate) fn get(&self, reference: u32) -> Result<&DragRecord, i16> {
        self.records.get(&reference).ok_or(BAD_DRAG_REF_ERR)
    }

    pub(crate) fn get_mut(&mut self, reference: u32) -> Result<&mut DragRecord, i16> {
        self.records.get_mut(&reference).ok_or(BAD_DRAG_REF_ERR)
    }

    pub(crate) fn dispose(&mut self, reference: u32) -> Result<(), i16> {
        self.records
            .remove(&reference)
            .map(|_| ())
            .ok_or(BAD_DRAG_REF_ERR)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disposal_invalidates_reference_without_reusing_it() {
        let mut manager = DragManager::default();
        assert_eq!(manager.get(0), Err(BAD_DRAG_REF_ERR));
        let first = manager.new_drag().unwrap();
        manager.dispose(first).unwrap();
        assert_eq!(manager.get_mut(first), Err(BAD_DRAG_REF_ERR));
        assert_eq!(manager.dispose(first), Err(BAD_DRAG_REF_ERR));
        assert_ne!(manager.new_drag().unwrap(), first);
    }

    #[test]
    fn flavors_preserve_order_promises_and_partial_reads() {
        let mut record = DragRecord::default();
        let flavor = DragFlavor {
            flavor_type: 7,
            flags: 2,
            data: Some(vec![10, 20, 30]),
        };
        record.add_flavor(42, flavor.clone()).unwrap();
        assert_eq!(record.add_flavor(42, flavor), Err(DUPLICATE_FLAVOR_ERR));
        record
            .add_flavor(
                9,
                DragFlavor {
                    flavor_type: 8,
                    flags: 0,
                    data: None,
                },
            )
            .unwrap();
        assert_eq!(
            record
                .items
                .iter()
                .map(|item| item.reference)
                .collect::<Vec<_>>(),
            vec![42, 9]
        );
        assert_eq!(record.flavor_data(42, 7, 1, usize::MAX), Ok(&[20, 30][..]));
        assert_eq!(record.flavor_data(42, 7, 3, 10), Ok(&[][..]));
        assert_eq!(record.flavor_data(42, 7, 4, 10), Err(-50));
        assert_eq!(record.flavor_data(9, 8, 0, 10), Err(CANT_GET_FLAVOR_ERR));
        assert_eq!(record.flavor(42, 99), Err(BAD_DRAG_FLAVOR_ERR));
        assert_eq!(record.item(99), Err(BAD_DRAG_ITEM_ERR));
    }

    #[test]
    fn handlers_support_chains_and_do_not_outlive_their_windows() {
        let mut manager = DragManager::default();
        let global = DragHandler {
            kind: DragHandlerKind::Tracking,
            window: 0,
            procedure: 10,
            refcon: 1,
            rtoc: 2,
        };
        let local = DragHandler {
            window: 100,
            procedure: 11,
            ..global
        };
        let second = DragHandler {
            procedure: 12,
            ..local
        };
        let receiver = DragHandler {
            kind: DragHandlerKind::Receive,
            ..local
        };
        for handler in [global, local, second, receiver] {
            manager.install_handler(handler).unwrap();
        }
        assert_eq!(
            manager.install_handler(DragHandler {
                refcon: 99,
                ..local
            }),
            Err(DUPLICATE_HANDLER_ERR)
        );
        assert_eq!(
            manager.handlers_for_window(DragHandlerKind::Tracking, 100),
            vec![global, local, second]
        );
        assert_eq!(
            manager.handlers_for_window(DragHandlerKind::Tracking, 200),
            vec![global]
        );
        let callbacks = manager.handlers_for_window(DragHandlerKind::Receive, 100);
        manager
            .remove_handler(DragHandlerKind::Receive, 100, 11)
            .unwrap();
        assert_eq!(callbacks, vec![receiver]);
        assert_eq!(
            manager.remove_handler(DragHandlerKind::Receive, 100, 11),
            Err(HANDLER_NOT_FOUND_ERR)
        );
        manager.remove_window(100);
        manager.remove_window(0);
        assert_eq!(
            manager.handlers_for_window(DragHandlerKind::Tracking, 100),
            vec![global]
        );
        assert_eq!(
            manager.install_handler(DragHandler {
                procedure: 0,
                ..local
            }),
            Err(-50)
        );
    }
    #[test]
    fn global_tracking_handler_stays_entered_across_window_transitions() {
        let global = DragHandler {
            kind: DragHandlerKind::Tracking,
            window: 0,
            procedure: 10,
            refcon: 1,
            rtoc: 2,
        };
        let local = DragHandler {
            window: 100,
            procedure: 11,
            ..global
        };
        let initial = tracking_messages(None, &[], Some(100), &[global, local]);
        assert_eq!(
            initial
                .iter()
                .map(|entry| entry.message)
                .collect::<Vec<_>>(),
            vec![1, 1, 2, 2, 3, 3]
        );
        let transition = tracking_messages(Some(100), &[global, local], Some(200), &[global]);
        assert_eq!(
            transition
                .iter()
                .map(|entry| (entry.handler.procedure, entry.window, entry.message))
                .collect::<Vec<_>>(),
            vec![
                (10, 100, 4),
                (11, 100, 4),
                (11, 100, 5),
                (10, 200, 2),
                (10, 200, 3)
            ]
        );
        let idle = tracking_messages(Some(200), &[global], Some(200), &[global]);
        assert_eq!(
            idle.iter().map(|entry| entry.message).collect::<Vec<_>>(),
            vec![3]
        );
        let leave = tracking_messages(Some(200), &[global], None, &[]);
        assert_eq!(
            leave.iter().map(|entry| entry.message).collect::<Vec<_>>(),
            vec![4, 5]
        );
    }
    #[test]
    fn stationary_press_waits_and_release_does_not_start_a_drag() {
        assert_eq!(
            wait_mouse_moved((10, 20), (10, 20), true),
            MouseMoveWaitResult::Waiting
        );
        assert_eq!(
            wait_mouse_moved((10, 20), (10, 21), true),
            MouseMoveWaitResult::Moved
        );
        assert_eq!(
            wait_mouse_moved((10, 20), (11, 20), true),
            MouseMoveWaitResult::Moved
        );
        assert_eq!(
            wait_mouse_moved((10, 20), (10, 20), false),
            MouseMoveWaitResult::Released
        );
        assert_eq!(
            wait_mouse_moved((10, 20), (11, 20), false),
            MouseMoveWaitResult::Released
        );
    }
    #[test]
    fn item_bounds_follow_mouse_and_unset_bounds_are_empty_at_mouse() {
        let mut record = DragRecord::default();
        record
            .add_flavor(
                42,
                DragFlavor {
                    flavor_type: 7,
                    flags: 0,
                    data: Some(Vec::new()),
                },
            )
            .unwrap();
        record.origin = (100, 200);
        record.mouse = (112, 180);
        assert_eq!(record.item_bounds(42), Ok((112, 180, 112, 180)));
        record.item_mut(42).unwrap().bounds = Some((90, 190, 110, 210));
        assert_eq!(record.item_bounds(42), Ok((102, 170, 122, 190)));
        record.mouse = record.origin;
        assert_eq!(record.item_bounds(42), Ok((90, 190, 110, 210)));
        assert_eq!(record.item_bounds(99), Err(BAD_DRAG_ITEM_ERR));
    }
    #[test]
    fn zoom_rect_path_clamps_steps_and_interpolates_signed_endpoints() {
        for (steps, expected) in [(-1, 4), (4, 4), (9, 9), (30, 25)] {
            let path =
                ZoomRectPath::new([-100, 50, 40, 200], [100, -50, 80, -100], steps, 0).unwrap();
            assert_eq!(path.steps(), expected);
            assert_eq!(path.frame(0), [-100, 50, 40, 200]);
            assert_eq!(path.frame(expected), [100, -50, 80, -100]);
            assert_eq!(path.frame(u16::MAX), [100, -50, 80, -100]);
        }
        let linear = ZoomRectPath::new([0; 4], [100; 4], 4, 0).unwrap();
        let fast = ZoomRectPath::new([0; 4], [100; 4], 4, 1).unwrap();
        let slow = ZoomRectPath::new([0; 4], [100; 4], 4, 2).unwrap();
        assert!(fast.frame(1)[0] < linear.frame(1)[0]);
        assert!(slow.frame(1)[0] > linear.frame(1)[0]);
        assert!(ZoomRectPath::new([0; 4], [0; 4], 4, 3).is_none());
    }

    #[test]
    fn hilite_is_two_pixels_and_excludes_the_region_boundary() {
        let rect = |v, h| (10..20).contains(&v) && (10..20).contains(&h);
        assert!(!hilite_pixel((10, 15), true, rect));
        assert!(hilite_pixel((11, 15), true, rect));
        assert!(hilite_pixel((12, 15), true, rect));
        assert!(!hilite_pixel((13, 15), true, rect));
        assert!(!hilite_pixel((9, 15), false, rect));
        assert!(hilite_pixel((8, 15), false, rect));
        assert!(hilite_pixel((7, 15), false, rect));
        assert!(!hilite_pixel((6, 15), false, rect));
        assert!(!hilite_pixel((15, 15), false, rect));
    }
}
