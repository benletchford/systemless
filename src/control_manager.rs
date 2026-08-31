//! Architecture-neutral Control Manager records and list operations.

/// Maximum number of controls accepted from one live guest `wControlList`.
///
/// This is a defensive corruption bound, not a guest-visible Control Manager
/// limit. A repeated handle terminates traversal before this bound is reached.
const MAX_CONTROL_LIST_ENTRIES: usize = 4096;

/// Resolve the handles that `DrawControls` must present, in draw order.
///
/// `NewControl` prepends records to `wControlList`. `DrawControls` draws in
/// reverse order of creation, so the architecture-neutral manager reverses
/// that newest-first guest chain and returns the first-created control first.
/// CPU adapters remain responsible only for reading the live next-handle field
/// and presenting or invoking the resulting control. Macintosh Toolbox
/// Essentials (1992), pp. 5-82 and 5-87--5-88.
pub(crate) fn control_draw_order<Handle>(
    head: Handle,
    mut next: impl FnMut(Handle) -> Option<Handle>,
) -> Vec<Handle>
where
    Handle: Copy + Eq + Default,
{
    let nil = Handle::default();
    let mut newest_first = Vec::new();
    let mut handle = head;
    while handle != nil
        && newest_first.len() < MAX_CONTROL_LIST_ENTRIES
        && !newest_first.contains(&handle)
    {
        newest_first.push(handle);
        handle = next(handle).unwrap_or(nil);
    }
    newest_first.reverse();
    newest_first
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn draw_order_reverses_the_newest_first_guest_chain() {
        let next = HashMap::from([(3u32, 2u32), (2, 1), (1, 0)]);
        assert_eq!(
            control_draw_order(3, |handle| next.get(&handle).copied()),
            [1, 2, 3]
        );
    }

    #[test]
    fn draw_order_stops_at_a_corrupt_cycle() {
        let next = HashMap::from([(3u32, 2u32), (2, 3)]);
        assert_eq!(
            control_draw_order(3, |handle| next.get(&handle).copied()),
            [2, 3]
        );
    }
}
