//! Bounded live frontend CPU work, also exercised by headless scheduling tests.

use std::time::{Duration, Instant};
use systemless::systems::macintosh::{game, runner::FixtureRunner};

pub(crate) fn advance(
    runner: &mut FixtureRunner,
    batch: usize,
    guest_deadline: u32,
    host_deadline: Instant,
) -> usize {
    let mut steps = 0;
    while runner.guest_tick() < guest_deadline && steps < game::MAX_INSTRUCTIONS_PER_FRAME {
        let host_remaining = host_deadline.saturating_duration_since(Instant::now());
        if host_remaining.is_zero() {
            break;
        }
        // Check deadlines more often near the end of the host budget. A full
        // 10,000-instruction batch can turn EV's 12 ms slice into a late frame.
        // Guest tick/instruction limits are unchanged; individual Toolbox traps
        // still finish atomically and can exceed the remaining host time.
        let slice_batch = if host_remaining < Duration::from_millis(2) {
            batch.min(1_000)
        } else {
            batch
        };
        let remaining = game::MAX_INSTRUCTIONS_PER_FRAME - steps;
        let (executed, running) =
            runner.run_gui_cpu_slice(slice_batch.min(remaining), guest_deadline);
        steps += executed;
        if executed == 0 || !running || runner.is_ui_tracking_active() {
            break;
        }
    }
    steps
}
