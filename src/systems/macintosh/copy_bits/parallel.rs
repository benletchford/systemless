//! Native-only workers for immutable, already-snapshotted indexed rows.
//! Guest memory and ordered tracked writes never enter this module.

use super::Indexed8HorizontalShrink;
use std::cell::RefCell;
use std::ops::Range;
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};

const MINIMUM_SNAPSHOT_BYTES: usize = 256 * 1024;

struct Work {
    plan: Indexed8HorizontalShrink,
    snapshots: Vec<u8>,
}

impl Work {
    fn compute(&self, rows: Range<usize>) -> Option<Vec<u8>> {
        let stride = self.plan.source_range().len();
        let start = rows.start.checked_mul(stride)?;
        let end = rows.end.checked_mul(stride)?;
        self.plan
            .reduce_rows(self.snapshots.get(start..end)?, rows.len())
    }
}

struct Job {
    work: Arc<Work>,
    rows: Range<usize>,
}

struct Worker {
    sender: mpsc::SyncSender<Option<Job>>,
    results: mpsc::Receiver<Option<Vec<u8>>>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    fn new(index: usize) -> std::io::Result<Self> {
        let (sender, jobs) = mpsc::sync_channel::<Option<Job>>(1);
        let (completed, results) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name(format!("systemless-graphics-{index}"))
            .spawn(move || {
                while let Ok(Some(job)) = jobs.recv() {
                    if completed.send(job.work.compute(job.rows)).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            sender,
            results,
            thread: Some(thread),
        })
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.sender.send(None);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(super) struct KernelPool {
    workers: Vec<Worker>,
    broken: bool,
}

fn partition(rows: usize, participants: usize, index: usize) -> Range<usize> {
    let base = rows / participants;
    let extra = rows % participants;
    let start = base * index + index.min(extra);
    start..start + base + usize::from(index < extra)
}

impl KernelPool {
    pub(super) fn new(participants: usize) -> std::io::Result<Self> {
        assert!(matches!(participants, 1 | 2 | 4));
        let mut workers = Vec::new();
        for index in 1..participants {
            workers.push(Worker::new(index)?);
        }
        Ok(Self {
            workers,
            broken: false,
        })
    }

    fn reduce(
        &mut self,
        plan: Indexed8HorizontalShrink,
        snapshots: Vec<u8>,
        rows: usize,
    ) -> Option<Vec<u8>> {
        let participants = self.workers.len() + 1;
        if self.broken || participants == 1 || rows < participants {
            return plan.reduce_rows(&snapshots, rows);
        }
        if snapshots.len() != rows.checked_mul(plan.source_range().len())? {
            return None;
        }
        let output_len = rows.checked_mul(plan.groups.len())?;
        let mut output = Vec::new();
        output.try_reserve_exact(output_len).ok()?;
        let work = Arc::new(Work { plan, snapshots });
        let mut sent = Vec::with_capacity(self.workers.len());
        for (index, worker) in self.workers.iter().enumerate() {
            let accepted = worker
                .sender
                .send(Some(Job {
                    work: work.clone(),
                    rows: partition(rows, participants, index),
                }))
                .is_ok();
            sent.push(accepted);
            self.broken |= !accepted;
        }
        // The owner is a compute participant, not a waiting coordinator.
        let owner_output = work.compute(partition(rows, participants, participants - 1));
        for (worker, accepted) in self.workers.iter().zip(sent) {
            if accepted {
                match worker.results.recv() {
                    Ok(Some(bytes)) => output.extend_from_slice(&bytes),
                    _ => self.broken = true,
                }
            }
        }
        if let Some(bytes) = owner_output {
            output.extend_from_slice(&bytes);
        } else {
            self.broken = true;
        }
        // All accepted jobs have been drained before fallback. No guest write
        // has happened, so retrying the pure calculation cannot replay a commit.
        if self.broken {
            return work.plan.reduce_rows(&work.snapshots, rows);
        }
        (output.len() == output_len).then_some(output)
    }
}

struct State {
    participants: usize,
    threshold: usize,
    pool: Option<KernelPool>,
}

impl State {
    fn configured() -> Self {
        let participants = std::env::var("SYSTEMLESS_GRAPHICS_PARTICIPANTS")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|value| matches!(value, 1 | 2 | 4))
            .unwrap_or(1);
        Self {
            participants,
            threshold: MINIMUM_SNAPSHOT_BYTES,
            pool: None,
        }
    }
}

thread_local! {
    // One pool per calling owner. Neither a runner nor guest memory is shared.
    // Workers are reused across operations and joined when that owner exits.
    static STATE: RefCell<State> = RefCell::new(State::configured());
}

pub(super) fn reduce(
    plan: Indexed8HorizontalShrink,
    snapshots: Vec<u8>,
    rows: usize,
) -> Option<Vec<u8>> {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.participants == 1 || snapshots.len() < state.threshold || rows < state.participants
        {
            return plan.reduce_rows(&snapshots, rows);
        }
        if state.pool.is_none() {
            match KernelPool::new(state.participants) {
                Ok(pool) => state.pool = Some(pool),
                Err(_) => {
                    state.participants = 1;
                    return plan.reduce_rows(&snapshots, rows);
                }
            }
        }
        state.pool.as_mut()?.reduce(plan, snapshots, rows)
    })
}

#[cfg(test)]
pub(super) fn with_participants<T>(participants: usize, body: impl FnOnce() -> T) -> T {
    struct Restore(Option<State>);
    impl Drop for Restore {
        fn drop(&mut self) {
            STATE.with(|state| {
                *state.borrow_mut() = self.0.take().unwrap();
            });
        }
    }
    let previous = STATE.with(|state| {
        state.replace(State {
            participants,
            threshold: 0,
            pool: None,
        })
    });
    let _restore = Restore(Some(previous));
    body()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::copy_bits::{
        BytePixmap, CopyBitsMemory, Indexed8ScalingSelection, RowCopy, RowCopyOutcome,
    };

    #[test]
    fn persistent_pool_matches_scalar_clipped_guard_and_identity_map_rows() {
        for participants in [1, 2, 4] {
            let mut pool = KernelPool::new(participants).unwrap();
            let threads: Vec<_> = pool
                .workers
                .iter()
                .map(|worker| worker.thread.as_ref().unwrap().thread().id())
                .collect();
            for (width, destination) in [(5, 3), (190, 1), (1025, 513), (32767, 1)] {
                for visible in [0..destination, destination / 2..destination, 0..0] {
                    for rows in [0, 1, 3, 4, 17] {
                        let plan =
                            Indexed8HorizontalShrink::new(width, destination, visible.clone())
                                .unwrap();
                        let snapshots: Vec<_> = (0..plan.source_range().len() * rows)
                            .map(|index| ((index * 73 + rows) % 256) as u8)
                            .collect();
                        let expected = plan.reduce_rows(&snapshots, rows);
                        assert_eq!(pool.reduce(plan, snapshots, rows), expected, "participants={participants}, width={width}, visible={visible:?}, rows={rows}");
                    }
                }
            }
            assert!(!pool.broken);
            assert_eq!(
                threads,
                pool.workers
                    .iter()
                    .map(|worker| worker.thread.as_ref().unwrap().thread().id())
                    .collect::<Vec<_>>()
            );
            // An invalid request must not leave stale work/results in the pool.
            let plan = Indexed8HorizontalShrink::new(8, 4, 0..4).unwrap();
            assert_eq!(pool.reduce(plan, vec![0; 31], 4), None);
            let plan = Indexed8HorizontalShrink::new(8, 4, 0..4).unwrap();
            assert_eq!(pool.reduce(plan, vec![9; 32], 4), Some(vec![9; 16]));
        }
    }

    #[test]
    fn small_operations_do_not_create_workers() {
        with_participants(4, || {
            STATE.with(|state| state.borrow_mut().threshold = MINIMUM_SNAPSHOT_BYTES);
            let small = Indexed8HorizontalShrink::new(64, 32, 0..32).unwrap();
            assert_eq!(reduce(small, vec![3; 64 * 64], 64), Some(vec![3; 32 * 64]));
            STATE.with(|state| assert!(state.borrow().pool.is_none()));
            let large = Indexed8HorizontalShrink::new(512, 256, 0..256).unwrap();
            assert_eq!(
                reduce(large, vec![5; 512 * 512], 512),
                Some(vec![5; 256 * 512])
            );
            STATE.with(|state| assert_eq!(state.borrow().pool.as_ref().unwrap().workers.len(), 3));
        });
    }

    #[test]
    fn disconnected_worker_falls_back_before_commit_and_stays_serial() {
        let mut pool = KernelPool::new(4).unwrap();
        pool.workers[1].sender.send(None).unwrap();
        pool.workers[1].thread.take().unwrap().join().unwrap();
        for value in [7, 99] {
            let plan = Indexed8HorizontalShrink::new(8, 4, 0..4).unwrap();
            assert_eq!(
                pool.reduce(plan, vec![value; 136], 17),
                Some(vec![value; 68])
            );
            assert!(pool.broken);
        }
    }

    struct Memory {
        bytes: Vec<u8>,
        events: Vec<(bool, u32)>,
        fail_read: Option<u32>,
        fail_write: Option<u32>,
        owner: thread::ThreadId,
    }

    impl CopyBitsMemory for Memory {
        fn read_copy_row(&mut self, address: u32, bytes: &mut [u8]) -> Option<()> {
            assert_eq!(thread::current().id(), self.owner);
            self.events.push((false, address));
            if self.fail_read == Some(address) {
                return None;
            }
            bytes.copy_from_slice(
                self.bytes
                    .get(address as usize..address as usize + bytes.len())?,
            );
            Some(())
        }
        fn write_copy_row(&mut self, address: u32, bytes: &[u8]) -> Option<()> {
            assert_eq!(thread::current().id(), self.owner);
            self.events.push((true, address));
            if self.fail_write == Some(address) {
                return None;
            }
            self.bytes
                .get_mut(address as usize..address as usize + bytes.len())?
                .copy_from_slice(bytes);
            Some(())
        }
    }

    #[test]
    fn full_operation_preserves_aliasing_clipping_owner_and_partial_write_order() {
        const SOURCE: u32 = 64;
        // Destination overlaps later source rows; every source must be captured
        // before committing even the first destination row.
        const DESTINATION: u32 = SOURCE + 32;
        for fail_read in [None, Some(SOURCE + 7 * 8 + 2)] {
            for fail_write in [None, Some(DESTINATION + 1), Some(DESTINATION + 7 * 4 + 1)] {
                let run = |participants| {
                    with_participants(participants, || {
                        let mut memory = Memory {
                            bytes: (0..512).map(|index| (index * 73 % 256) as u8).collect(),
                            events: Vec::new(),
                            fail_read,
                            fail_write,
                            owner: thread::current().id(),
                        };
                        let copy = RowCopy {
                            mode: 0,
                            source: BytePixmap {
                                base: SOURCE,
                                row_bytes: 8,
                                depth: 8,
                                bounds: [0, 0, 17, 7],
                            },
                            destination: BytePixmap {
                                base: DESTINATION,
                                row_bytes: 4,
                                depth: 8,
                                bounds: [0, 0, 17, 3],
                            },
                            source_rect: [0, 0, 17, 7],
                            destination_rect: [0, 0, 17, 3],
                            clip: [0, 1, 17, 3],
                            palette: None,
                        };
                        let outcome = copy.execute_with_indexed8_scaling(
                            &mut memory,
                            Indexed8ScalingSelection::from_adapter_facts(true, true, true, true, 0),
                        );
                        (outcome, memory.bytes, memory.events)
                    })
                };
                let expected = run(1);
                assert_eq!(
                    expected.0,
                    if fail_read.is_some() {
                        RowCopyOutcome::ReadOrGeometryFailure
                    } else if fail_write == Some(DESTINATION + 1) {
                        RowCopyOutcome::WriteFailure { rows_written: 0 }
                    } else if fail_write.is_some() {
                        RowCopyOutcome::WriteFailure { rows_written: 7 }
                    } else {
                        RowCopyOutcome::Completed
                    }
                );
                for participants in [2, 4] {
                    assert_eq!(
                        run(participants),
                        expected,
                        "participants={participants}, read={fail_read:?}, write={fail_write:?}"
                    );
                }
            }
        }
    }
}
