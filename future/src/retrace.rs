//! Shared system and virtual-main-display vertical retrace queues.
//! IM: Processes, Vertical Retrace Manager, pp. 4-6–4-8, 4-22–4-29.
use crate::{
    memory::{GuestPtr, Memory, Permissions},
    Error, Result,
};
use std::collections::VecDeque;
pub const SYSTEM: GuestPtr = GuestPtr(0x160);
pub const SCREEN_QUEUE: GuestPtr = GuestPtr(0x7120_0000);
pub const SCREEN_POINTER: GuestPtr = GuestPtr(0xd10);
const PERIOD: u64 = 50_000; // Existing virtual clock units: one 60 Hz refresh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Op {
    pub task: GuestPtr,
    pub slot: Option<i16>,
    pub remove: bool,
}
#[derive(Clone, Copy)]
struct Task {
    pointer: GuestPtr,
    queue: usize,
    count: u16,
    start: u64,
}
#[derive(Clone, Copy)]
pub(crate) struct Due {
    pub task: GuestPtr,
    queue: usize,
}
#[derive(Default)]
pub struct Retrace {
    tasks: Vec<Task>,
    pending: VecDeque<Due>,
    active: Option<Due>,
    tick: u64,
}
fn bad(s: &str) -> Error {
    Error::Cpu(format!("Vertical Retrace Manager: {s}"))
}
fn header(queue: usize) -> GuestPtr {
    if queue == 0 {
        SYSTEM
    } else {
        SCREEN_QUEUE
    }
}
fn overlaps(a: GuestPtr, an: u32, b: GuestPtr, bn: u32) -> bool {
    u64::from(a.0) < u64::from(b.0) + u64::from(bn)
        && u64::from(b.0) < u64::from(a.0) + u64::from(an)
}
pub(crate) fn install(m: &mut Memory) -> Result<()> {
    m.map(SCREEN_QUEUE, vec![0; 10], Permissions::RW)?;
    m.write(SCREEN_POINTER, &SCREEN_QUEUE.0.to_be_bytes())
}
impl Retrace {
    fn record(m: &Memory, p: GuestPtr) -> Result<[u8; 14]> {
        if p.0 == 0 || !p.0.is_multiple_of(2) {
            return Err(bad("invalid task pointer"));
        }
        Ok(m.read(p, 14)?.try_into().unwrap())
    }
    pub(crate) fn overlaps(&self, p: GuestPtr) -> bool {
        self.tasks.iter().any(|t| overlaps(t.pointer, 14, p, 14))
            || [SYSTEM, SCREEN_QUEUE, SCREEN_POINTER]
                .into_iter()
                .any(|h| overlaps(p, 14, h, if h == SCREEN_POINTER { 4 } else { 10 }))
    }
    fn validate(&self, m: &Memory) -> Result<()> {
        if m.u32(SCREEN_POINTER)? != SCREEN_QUEUE.0 {
            return Err(bad("guest changed screen queue pointer"));
        }
        for queue in 0..2 {
            let tasks: Vec<_> = self.tasks.iter().filter(|t| t.queue == queue).collect();
            let h = m.read(header(queue), 10)?;
            let active = self.active.is_some_and(|d| d.queue == queue);
            if (h[1] & 0x40 != 0) != active
                || h[2..6] != tasks.first().map_or(0, |t| t.pointer.0).to_be_bytes()
                || h[6..10] != tasks.last().map_or(0, |t| t.pointer.0).to_be_bytes()
            {
                return Err(bad("guest changed managed queue header"));
            }
            for (i, t) in tasks.iter().enumerate() {
                let r = Self::record(m, t.pointer)?;
                if r[..4] != tasks.get(i + 1).map_or(0, |n| n.pointer.0).to_be_bytes()
                    || r[4..6] != 1u16.to_be_bytes()
                {
                    return Err(bad("guest changed managed task linkage or type"));
                }
            }
        }
        Ok(())
    }
    fn relink(&self, m: &mut Memory, queue: usize) -> Result<()> {
        let tasks: Vec<_> = self.tasks.iter().filter(|t| t.queue == queue).collect();
        m.validate_write(header(queue), 10)?;
        for t in &tasks {
            m.validate_write(t.pointer, 4)?;
        }
        for (i, t) in tasks.iter().enumerate() {
            m.write(
                t.pointer,
                &tasks.get(i + 1).map_or(0, |n| n.pointer.0).to_be_bytes(),
            )?;
        }
        m.write(
            GuestPtr(header(queue).0 + 2),
            &tasks.first().map_or(0, |t| t.pointer.0).to_be_bytes(),
        )?;
        m.write(
            GuestPtr(header(queue).0 + 6),
            &tasks.last().map_or(0, |t| t.pointer.0).to_be_bytes(),
        )
    }
    pub fn call(&mut self, m: &mut Memory, op: Op, now: u64) -> Result<i16> {
        let queue = match op.slot {
            None => 0,
            Some(0) => 1,
            Some(_) => return Ok(-360),
        };
        let r = Self::record(m, op.task)?;
        if r[4..6] != 1u16.to_be_bytes() {
            return Ok(-2);
        }
        self.validate(m)?;
        if self.active.is_some() {
            return Err(bad("queue mutation from a VBL callback is unsupported"));
        }
        let at = self
            .tasks
            .iter()
            .position(|t| t.pointer == op.task && t.queue == queue);
        if op.remove {
            let Some(at) = at else {
                return Ok(-1);
            };
            m.validate_write(op.task, 4)?;
            m.validate_write(header(queue), 10)?;
            for t in self.tasks.iter().filter(|t| t.queue == queue) {
                m.validate_write(t.pointer, 4)?;
            }
            self.tasks.remove(at);
            self.pending.retain(|d| d.task != op.task);
            m.write(op.task, &0u32.to_be_bytes())?;
            self.relink(m, queue)?;
            return Ok(0);
        }
        if self.overlaps(op.task) {
            return Err(bad("duplicate or overlapping task record"));
        }
        if self.tasks.len() >= 256 {
            return Err(bad("task limit exceeded"));
        }
        let count = u16::from_be_bytes(r[10..12].try_into().unwrap());
        let phase = u16::from_be_bytes(r[12..14].try_into().unwrap());
        // These are guest machine words. The one-time phase addition and
        // countdown retain 16-bit arithmetic rather than rejecting signed wrap.
        let initial = count.wrapping_add(phase);
        m.validate_write(op.task, 14)?;
        m.validate_write(header(queue), 10)?;
        for t in self.tasks.iter().filter(|t| t.queue == queue) {
            m.validate_write(t.pointer, 4)?;
        }
        if self.tasks.is_empty() {
            self.tick = now / PERIOD;
        }
        self.tasks.push(Task {
            pointer: op.task,
            queue,
            count: initial,
            start: now / PERIOD,
        });
        m.write(GuestPtr(op.task.0 + 10), &initial.to_be_bytes())?;
        self.relink(m, queue)?;
        Ok(0)
    }
    pub(crate) fn wait_ticks(&self) -> Option<u32> {
        (!self.tasks.is_empty()).then_some(1)
    }
    pub(crate) fn pending(&mut self, m: &mut Memory, now: u64) -> Result<Option<Due>> {
        if self.tasks.is_empty() {
            return Ok(None);
        }
        self.validate(m)?;
        let now = now / PERIOD;
        // A foreground edit of the live countdown starts at the observed current
        // refresh, not at an older unserviced interrupt. Callback rearming is
        // finalized separately at its own interrupt's timestamp.
        for t in &mut self.tasks {
            let count = m.u16(GuestPtr(t.pointer.0 + 10))?;
            if count != t.count {
                t.count = count;
                if !self.pending.iter().any(|d| d.task == t.pointer) {
                    t.start = now;
                }
            }
        }
        loop {
            // Walk the final refresh sequentially. An earlier task may edit a
            // later task's live count, so do not pre-decrement every task before
            // the first callback has run.
            while let Some(due) = self.pending.front().copied() {
                let task = self
                    .tasks
                    .iter_mut()
                    .find(|t| t.pointer == due.task)
                    .ok_or_else(|| bad("lost refresh scan task"))?;
                let count = m.u16(GuestPtr(task.pointer.0 + 10))?;
                if count == 1 {
                    return Ok(Some(due));
                }
                if count > 1 {
                    m.write(GuestPtr(task.pointer.0 + 10), &(count - 1).to_be_bytes())?;
                    task.count = count - 1;
                }
                self.pending.pop_front();
            }
            let next = self
                .tasks
                .iter()
                .filter(|t| t.count > 0)
                .map(|t| self.tick.max(t.start) + t.count as u64)
                .min()
                .unwrap_or(now);
            let target = next.min(now);
            if target < self.tick {
                return Err(bad("refresh clock moved backward"));
            }
            if target == self.tick {
                return Ok(None);
            }
            for t in &self.tasks {
                m.validate_write(GuestPtr(t.pointer.0 + 10), 2)?;
            }
            // Skip refreshes with no possible callback, retaining one final
            // decrement for the ordered scan at the target interrupt.
            for queue in 0..2 {
                for t in self.tasks.iter_mut().filter(|t| t.queue == queue) {
                    let elapsed = target.saturating_sub(self.tick.max(t.start));
                    if elapsed == 0 {
                        continue;
                    }
                    if t.count > 0 {
                        let skipped = elapsed.saturating_sub(1).min(t.count as u64);
                        t.count -= skipped as u16;
                        m.write(GuestPtr(t.pointer.0 + 10), &t.count.to_be_bytes())?;
                    }
                    self.pending.push_back(Due {
                        task: t.pointer,
                        queue,
                    });
                }
            }
            self.tick = target;
        }
    }
    pub(crate) fn begin(&mut self, m: &mut Memory, due: Due) -> Result<()> {
        self.validate(m)?;
        if self.active.is_some() || self.pending.front().is_none_or(|d| d.task != due.task) {
            return Err(bad("invalid callback ownership"));
        }
        let h = header(due.queue);
        let flags = m.u16(h)?;
        m.validate_write(h, 2)?;
        m.validate_write(GuestPtr(due.task.0 + 10), 2)?;
        m.write(GuestPtr(due.task.0 + 10), &0u16.to_be_bytes())?;
        self.tasks
            .iter_mut()
            .find(|t| t.pointer == due.task)
            .unwrap()
            .count = 0;
        m.write(h, &(flags | 0x40).to_be_bytes())?;
        self.pending.pop_front();
        self.active = Some(due);
        Ok(())
    }
    pub(crate) fn finish(&mut self, m: &mut Memory, task: GuestPtr) -> Result<()> {
        self.validate(m)?;
        let due = self
            .active
            .ok_or_else(|| bad("callback returned without active task"))?;
        if due.task != task {
            return Err(bad("callback owner mismatch"));
        }
        let at = self
            .tasks
            .iter()
            .position(|t| t.pointer == task)
            .ok_or_else(|| bad("lost callback task"))?;
        let count = m.u16(GuestPtr(task.0 + 10))?;
        let h = header(due.queue);
        m.validate_write(h, 10)?;
        m.validate_write(task, 4)?;
        for t in self.tasks.iter().filter(|t| t.queue == due.queue) {
            m.validate_write(t.pointer, 4)?;
        }
        if count == 0 {
            self.tasks.remove(at);
            m.write(task, &0u32.to_be_bytes())?;
        } else {
            self.tasks[at].count = count;
            self.tasks[at].start = self.tick;
        }
        self.relink(m, due.queue)?;
        let flags = m.u16(h)?;
        m.write(h, &(flags & !0x40).to_be_bytes())?;
        self.active = None;
        Ok(())
    }
    pub(crate) fn cancel(&mut self, m: &mut Memory) {
        // Process termination cannot replace an earlier fault with cleanup errors.
        for t in &self.tasks {
            let _ = m.write(t.pointer, &0u32.to_be_bytes());
        }
        for h in [SYSTEM, SCREEN_QUEUE] {
            let _ = m.write(h, &[0; 10]);
        }
        self.tasks.clear();
        self.pending.clear();
        self.active = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn memory() -> Memory {
        let mut m = Memory::default();
        m.map(GuestPtr(0), vec![0; 4096], Permissions::RW).unwrap();
        install(&mut m).unwrap();
        m.map(GuestPtr(0x10000), vec![0; 128], Permissions::RW)
            .unwrap();
        m
    }
    fn task(m: &mut Memory, p: u32, count: i16, phase: i16) {
        let mut r = [0; 14];
        r[4..6].copy_from_slice(&1u16.to_be_bytes());
        r[6..10].copy_from_slice(&0x20000u32.to_be_bytes());
        r[10..12].copy_from_slice(&count.to_be_bytes());
        r[12..14].copy_from_slice(&phase.to_be_bytes());
        m.write(GuestPtr(p), &r).unwrap();
    }
    fn op(p: u32, slot: Option<i16>, remove: bool) -> Op {
        Op {
            task: GuestPtr(p),
            slot,
            remove,
        }
    }
    #[test]
    fn phase_is_initial_only_and_zero_rearm_removes_real_queue_links() {
        let mut m = memory();
        let mut v = Retrace::default();
        task(&mut m, 0x10000, 3, 1);
        assert_eq!(v.call(&mut m, op(0x10000, None, false), 0).unwrap(), 0);
        assert_eq!(m.u16(GuestPtr(0x1000a)).unwrap(), 4);
        assert!(v.pending(&mut m, 3 * PERIOD).unwrap().is_none());
        let due = v.pending(&mut m, 4 * PERIOD).unwrap().unwrap();
        v.begin(&mut m, due).unwrap();
        assert_eq!(m.u16(SYSTEM).unwrap() & 0x40, 0x40);
        m.write(GuestPtr(0x1000a), &2u16.to_be_bytes()).unwrap();
        v.finish(&mut m, due.task).unwrap();
        assert_eq!(m.u16(SYSTEM).unwrap() & 0x40, 0);
        assert!(v.pending(&mut m, 5 * PERIOD).unwrap().is_none());
        let due = v.pending(&mut m, 6 * PERIOD).unwrap().unwrap();
        v.begin(&mut m, due).unwrap();
        v.finish(&mut m, due.task).unwrap();
        assert_eq!(m.read(SYSTEM, 10).unwrap(), [0; 10]);
        assert!(v.tasks.is_empty());
    }
    #[test]
    fn dormant_tasks_observe_live_count_edits_without_replaying_past_refreshes() {
        let mut m = memory();
        let mut v = Retrace::default();
        task(&mut m, 0x10000, 0, 0);
        v.call(&mut m, op(0x10000, Some(0), false), 0).unwrap();
        assert!(v.pending(&mut m, 100 * PERIOD).unwrap().is_none());
        m.write(GuestPtr(0x1000a), &1u16.to_be_bytes()).unwrap();
        assert!(v.pending(&mut m, 100 * PERIOD).unwrap().is_none());
        let due = v.pending(&mut m, 101 * PERIOD).unwrap().unwrap();
        v.begin(&mut m, due).unwrap();
        assert_eq!(m.u16(SCREEN_QUEUE).unwrap() & 0x40, 0x40);
        assert_eq!(m.u16(SYSTEM).unwrap(), 0);
        v.finish(&mut m, due.task).unwrap();
        assert_eq!(m.read(SCREEN_QUEUE, 10).unwrap(), [0; 10]);
    }
    #[test]
    fn queue_ownership_order_errors_and_failed_writes_are_explicit() {
        let mut m = memory();
        let mut v = Retrace::default();
        task(&mut m, 0x10000, 1, 0);
        task(&mut m, 0x10020, 1, 0);
        assert_eq!(v.call(&mut m, op(0, Some(1), false), 0).unwrap(), -360);
        m.write(GuestPtr(0x10004), &2u16.to_be_bytes()).unwrap();
        assert_eq!(v.call(&mut m, op(0x10000, None, false), 0).unwrap(), -2);
        task(&mut m, 0x10000, 1, 0);
        v.call(&mut m, op(0x10000, None, false), 0).unwrap();
        v.call(&mut m, op(0x10020, None, false), 0).unwrap();
        assert_eq!(m.u32(GuestPtr(0x10000)).unwrap(), 0x10020);
        assert_eq!(v.call(&mut m, op(0x10000, Some(0), true), 0).unwrap(), -1);
        assert!(v.call(&mut m, op(0x10000, Some(0), false), 0).is_err());
        let first = v.pending(&mut m, PERIOD).unwrap().unwrap();
        assert_eq!(first.task.0, 0x10000);
        v.begin(&mut m, first).unwrap();
        v.finish(&mut m, first.task).unwrap();
        let second = v.pending(&mut m, PERIOD).unwrap().unwrap();
        assert_eq!(second.task.0, 0x10020);
        v.begin(&mut m, second).unwrap();
        v.finish(&mut m, second.task).unwrap();
        assert_eq!(m.read(SYSTEM, 10).unwrap(), [0; 10]);
        let mut bytes = [0; 14];
        bytes[4..6].copy_from_slice(&1u16.to_be_bytes());
        m.map(GuestPtr(0x20000), bytes.to_vec(), Permissions::RX)
            .unwrap();
        assert!(v.call(&mut m, op(0x20000, None, false), PERIOD).is_err());
        assert!(v.tasks.is_empty());
        assert_eq!(m.read(SYSTEM, 10).unwrap(), [0; 10]);
    }
    #[test]
    fn earlier_callbacks_can_change_or_disable_later_counts_in_the_same_refresh() {
        for (initial, changed) in [(5u16, 2u16), (1, 0)] {
            let mut m = memory();
            let mut v = Retrace::default();
            task(&mut m, 0x10000, 1, 0);
            task(&mut m, 0x10020, initial as i16, 0);
            v.call(&mut m, op(0x10000, None, false), 0).unwrap();
            v.call(&mut m, op(0x10020, None, false), 0).unwrap();
            let first = v.pending(&mut m, PERIOD).unwrap().unwrap();
            v.begin(&mut m, first).unwrap();
            assert_eq!(m.u16(GuestPtr(0x1002a)).unwrap(), initial);
            m.write(GuestPtr(0x1002a), &changed.to_be_bytes()).unwrap();
            v.finish(&mut m, first.task).unwrap();
            assert!(v.pending(&mut m, PERIOD).unwrap().is_none());
            assert_eq!(m.u16(GuestPtr(0x1002a)).unwrap(), changed.saturating_sub(1));
            if changed != 0 {
                let next = v.pending(&mut m, 2 * PERIOD).unwrap().unwrap();
                assert_eq!(next.task.0, 0x10020);
            }
        }
    }
    #[test]
    fn valid_initial_phase_sums_cross_the_signed_word_boundary_without_losing_the_task() {
        let mut m = memory();
        let mut v = Retrace::default();
        task(&mut m, 0x10000, 32767, 1);
        assert_eq!(v.call(&mut m, op(0x10000, None, false), 0).unwrap(), 0);
        assert_eq!(m.u16(GuestPtr(0x1000a)).unwrap(), 0x8000);
        assert!(v.pending(&mut m, 32767 * PERIOD).unwrap().is_none());
        assert_eq!(m.u16(GuestPtr(0x1000a)).unwrap(), 1);
        let due = v.pending(&mut m, 32768 * PERIOD).unwrap().unwrap();
        v.begin(&mut m, due).unwrap();
        v.finish(&mut m, due.task).unwrap();
        assert_eq!(m.read(SYSTEM, 10).unwrap(), [0; 10]);
    }
}
