//! Bounded live frontend CPU work, also exercised by headless scheduling tests.

use std::time::Instant;
use systemless::systems::macintosh::{game, runner::FixtureRunner};

pub(crate) fn advance(
    runner: &mut FixtureRunner,
    batch: usize,
    guest_deadline: u32,
    host_deadline: Instant,
) -> usize {
    let mut steps = 0;
    while runner.guest_tick() < guest_deadline
        && steps < game::MAX_INSTRUCTIONS_PER_FRAME
        && Instant::now() < host_deadline
    {
        let remaining = game::MAX_INSTRUCTIONS_PER_FRAME - steps;
        let (executed, running) = runner.run_gui_cpu_slice(batch.min(remaining), guest_deadline);
        steps += executed;
        if executed == 0 || !running || runner.is_ui_tracking_active() {
            break;
        }
    }
    steps
}
