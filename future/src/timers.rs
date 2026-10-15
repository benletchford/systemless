//! Shared revised Time Manager task ownership and deterministic deadlines.
//! Inside Macintosh: Processes, Time Manager, pp. 3-17–3-22.
//! Clock units are 1/3 microsecond: 50 per virtual instruction, 50,000 per tick.
use crate::{
    memory::{GuestPtr, Memory},
    Error, Result,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Insert,
    Prime(i32),
    Remove,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Op {
    pub task: GuestPtr,
    pub action: Action,
}
#[derive(Clone, Copy)]
struct Task {
    pointer: GuestPtr,
    deadline: Option<u64>,
}
#[derive(Default)]
pub struct Timers {
    now: u64,
    tasks: Vec<Task>,
}
#[derive(Clone, Copy)]
pub(crate) struct Due {
    pub task: GuestPtr,
    pub routine: u32,
}
fn bad(message: &str) -> Error {
    Error::Cpu(format!("Time Manager: {message}"))
}
impl Timers {
    pub(crate) fn advance(&mut self, units: u64) -> Result<()> {
        self.now = self
            .now
            .checked_add(units)
            .ok_or_else(|| bad("clock overflow"))?;
        Ok(())
    }
    pub(crate) fn validate_advance(&self, units: u64) -> Result<()> {
        self.now
            .checked_add(units)
            .ok_or_else(|| bad("clock overflow"))?;
        Ok(())
    }
    pub(crate) fn cancel(&mut self) {
        self.tasks.clear();
    }
    pub(crate) fn wait_ticks(&self) -> Option<u32> {
        self.tasks.iter().filter_map(|t| t.deadline).min().map(|d| {
            d.saturating_sub(self.now)
                .div_ceil(50_000)
                .max(1)
                .min(u64::from(u32::MAX)) as u32
        })
    }
    fn record(memory: &Memory, pointer: GuestPtr) -> Result<[u8; 14]> {
        if pointer.0 == 0 || !pointer.0.is_multiple_of(2) {
            return Err(bad("invalid task pointer"));
        }
        Ok(memory.read(pointer, 14)?.try_into().unwrap())
    }
    fn validate_queue(&self, memory: &Memory) -> Result<()> {
        for (index, task) in self.tasks.iter().enumerate() {
            let record = Self::record(memory, task.pointer)?;
            let next = self.tasks.get(index + 1).map_or(0, |t| t.pointer.0);
            if record[..4] != next.to_be_bytes()
                || (record[4] & 0x80 != 0) != task.deadline.is_some()
            {
                return Err(bad("guest changed managed queue linkage or active bit"));
            }
        }
        Ok(())
    }
    pub fn call(&mut self, memory: &mut Memory, op: Op) -> Result<()> {
        let mut record = Self::record(memory, op.task)?;
        self.validate_queue(memory)?;
        let at = self.tasks.iter().position(|t| t.pointer == op.task);
        match op.action {
            Action::Insert => {
                if at.is_some() {
                    return Err(bad("task is already installed"));
                }
                if self.tasks.iter().any(|t| {
                    u64::from(t.pointer.0) < u64::from(op.task.0) + 14
                        && u64::from(op.task.0) < u64::from(t.pointer.0) + 14
                }) {
                    return Err(bad("overlapping task records"));
                }
                if self.tasks.len() >= 256 {
                    return Err(bad("task limit exceeded"));
                }
                memory.validate_write(op.task, 6)?;
                if let Some(last) = self.tasks.last() {
                    memory.validate_write(last.pointer, 4)?;
                }
                record[..4].fill(0);
                record[4] &= 0x7f;
                if let Some(last) = self.tasks.last() {
                    memory.write(last.pointer, &op.task.0.to_be_bytes())?;
                }
                memory.write(op.task, &record[..6])?;
                self.tasks.push(Task {
                    pointer: op.task,
                    deadline: None,
                });
            }
            Action::Prime(count) => {
                let at = at.ok_or_else(|| bad("priming an uninstalled task"))?;
                if self.tasks[at].deadline.is_some() {
                    return Err(bad("repriming an unexpired task requires removal"));
                }
                let duration = if count < 0 {
                    u64::from(count.unsigned_abs()) * 3
                } else {
                    count as u64 * 3000
                };
                let deadline = self
                    .now
                    .checked_add(duration)
                    .ok_or_else(|| bad("deadline overflow"))?;
                memory.validate_write(GuestPtr(op.task.0 + 4), 2)?;
                record[4] |= 0x80;
                memory.write(GuestPtr(op.task.0 + 4), &record[4..6])?;
                self.tasks[at].deadline = Some(deadline);
            }
            Action::Remove => {
                let at = at.ok_or_else(|| bad("removing an uninstalled task"))?;
                let remaining = self.tasks[at]
                    .deadline
                    .map_or(0, |d| d.saturating_sub(self.now));
                let micros = remaining.div_ceil(3);
                let count = if micros <= i32::MAX as u64 {
                    -(micros as i32)
                } else {
                    remaining.div_ceil(3000).min(i32::MAX as u64) as i32
                };
                memory.validate_write(op.task, 14)?;
                if at > 0 {
                    memory.validate_write(self.tasks[at - 1].pointer, 4)?;
                }
                if at > 0 {
                    memory.write(self.tasks[at - 1].pointer, &record[..4])?;
                }
                record[..4].fill(0);
                record[4] &= 0x7f;
                record[10..14].copy_from_slice(&count.to_be_bytes());
                memory.write(op.task, &record)?;
                self.tasks.remove(at);
            }
        }
        Ok(())
    }
    pub(crate) fn pending(&self, memory: &Memory) -> Result<Option<Due>> {
        let Some(task) = self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.deadline.is_some_and(|d| d <= self.now))
            .min_by_key(|(i, t)| (t.deadline.unwrap(), *i))
            .map(|(_, t)| t)
        else {
            return Ok(None);
        };
        self.validate_queue(memory)?;
        let record = Self::record(memory, task.pointer)?;
        memory.validate_write(GuestPtr(task.pointer.0 + 4), 2)?;
        memory.validate_write(GuestPtr(task.pointer.0 + 10), 4)?;
        Ok(Some(Due {
            task: task.pointer,
            routine: u32::from_be_bytes(record[6..10].try_into().unwrap()),
        }))
    }
    pub(crate) fn expire(&mut self, memory: &mut Memory, due: Due) -> Result<()> {
        let at = self
            .tasks
            .iter()
            .position(|t| t.pointer == due.task)
            .ok_or_else(|| bad("lost due task"))?;
        let kind = memory.u16(GuestPtr(due.task.0 + 4))? & 0x7fff;
        memory.write(GuestPtr(due.task.0 + 4), &kind.to_be_bytes())?;
        memory.write(GuestPtr(due.task.0 + 10), &0i32.to_be_bytes())?;
        self.tasks[at].deadline = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Permissions;
    #[test]
    fn deadline_order_ties_nil_expiry_and_remaining_units_are_deterministic() {
        let mut m = Memory::default();
        m.map(GuestPtr(0x1000), vec![0; 128], Permissions::RW)
            .unwrap();
        let mut timers = Timers::default();
        for (pointer, delay) in [(0x1000, 2), (0x1020, 1), (0x1040, 1)] {
            timers
                .call(
                    &mut m,
                    Op {
                        task: GuestPtr(pointer),
                        action: Action::Insert,
                    },
                )
                .unwrap();
            timers
                .call(
                    &mut m,
                    Op {
                        task: GuestPtr(pointer),
                        action: Action::Prime(delay),
                    },
                )
                .unwrap();
        }
        timers.advance(6000).unwrap();
        for pointer in [0x1020, 0x1040, 0x1000] {
            let due = timers.pending(&m).unwrap().unwrap();
            assert_eq!(due.task.0, pointer);
            assert_eq!(due.routine, 0);
            timers.expire(&mut m, due).unwrap();
        }
        assert!(timers.pending(&m).unwrap().is_none());
        timers
            .call(
                &mut m,
                Op {
                    task: GuestPtr(0x1000),
                    action: Action::Prime(i32::MAX),
                },
            )
            .unwrap();
        timers.advance(1).unwrap();
        timers
            .call(
                &mut m,
                Op {
                    task: GuestPtr(0x1000),
                    action: Action::Remove,
                },
            )
            .unwrap();
        assert_eq!(m.u32(GuestPtr(0x100a)).unwrap(), i32::MAX as u32);
    }
    #[test]
    fn read_only_queue_links_overlapping_records_and_clock_overflow_do_not_mutate_state() {
        let mut m = Memory::default();
        m.map(GuestPtr(0x1000), vec![0; 64], Permissions::RW)
            .unwrap();
        let mut timers = Timers::default();
        timers
            .call(
                &mut m,
                Op {
                    task: GuestPtr(0x1000),
                    action: Action::Insert,
                },
            )
            .unwrap();
        let before = m.read(GuestPtr(0x1000), 64).unwrap().to_vec();
        assert!(timers
            .call(
                &mut m,
                Op {
                    task: GuestPtr(0x1002),
                    action: Action::Insert
                }
            )
            .is_err());
        assert_eq!(m.read(GuestPtr(0x1000), 64).unwrap(), before);
        let mut readonly = Memory::default();
        readonly
            .map(GuestPtr(0x1000), vec![0; 14], Permissions::RX)
            .unwrap();
        readonly
            .map(GuestPtr(0x1020), vec![0; 14], Permissions::RW)
            .unwrap();
        assert!(timers
            .call(
                &mut readonly,
                Op {
                    task: GuestPtr(0x1020),
                    action: Action::Insert
                }
            )
            .is_err());
        assert_eq!(readonly.read(GuestPtr(0x1020), 14).unwrap(), [0; 14]);
        assert_eq!(timers.tasks.len(), 1);
        timers.now = u64::MAX - 1;
        assert!(timers.advance(2).is_err());
        assert_eq!(timers.now, u64::MAX - 1);
    }
}
