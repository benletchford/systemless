//! Reusable resources for synchronous guest-visible GPU operations.
use std::cell::RefCell;
use std::collections::HashMap;

/// Borrow packed native u32 storage without allocating or serializing each
/// word. This is the same byte order as u32::to_ne_bytes used by presentation.
pub(super) fn word_bytes(words: &[u32]) -> &[u8] {
    // SAFETY: u32 has no padding and every byte is initialized. u8 accepts any
    // alignment and bit pattern. The returned borrow cannot outlive `words`.
    unsafe { std::slice::from_raw_parts(words.as_ptr().cast(), std::mem::size_of_val(words)) }
}

pub(super) fn little_endian_word_bytes(words: &[u32]) -> std::borrow::Cow<'_, [u8]> {
    #[cfg(target_endian = "little")]
    {
        std::borrow::Cow::Borrowed(word_bytes(words))
    }
    #[cfg(target_endian = "big")]
    {
        std::borrow::Cow::Owned(words.iter().flat_map(|word| word.to_le_bytes()).collect())
    }
}

const READBACK_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);
const METAL_SHORT_WAIT: std::time::Duration = std::time::Duration::from_micros(50);
const METAL_SHORT_BUDGET: std::time::Duration = std::time::Duration::from_millis(4);

/// Sleep in the backend while allowing Metal to observe completion even when
/// its separately scheduled notification is late. Every wait has a deadline;
/// a stalled compute queue must return control to guest CPU fallback.
pub(super) fn wait_for_readback(
    device: &wgpu::Device,
) -> Result<wgpu::PollStatus, wgpu::PollError> {
    let start = std::time::Instant::now();
    #[cfg(target_os = "macos")]
    // SAFETY: inspect only the backend type, retaining no HAL resource/guard.
    let metal = unsafe { device.as_hal::<wgpu::hal::api::Metal>() }.is_some();
    #[cfg(not(target_os = "macos"))]
    let metal = false;
    wait_with_budget(
        metal,
        || start.elapsed(),
        |timeout| {
            device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(timeout),
            })
        },
    )
}

fn wait_with_budget(
    metal: bool,
    mut elapsed: impl FnMut() -> std::time::Duration,
    mut poll: impl FnMut(std::time::Duration) -> Result<wgpu::PollStatus, wgpu::PollError>,
) -> Result<wgpu::PollStatus, wgpu::PollError> {
    if metal {
        while elapsed() < METAL_SHORT_BUDGET {
            match poll(METAL_SHORT_WAIT) {
                Err(wgpu::PollError::Timeout) => {}
                result => return result,
            }
        }
    }
    poll(READBACK_BUDGET.saturating_sub(elapsed()))
}

#[derive(Default)]
pub(super) struct ScratchBuffers {
    buffers: RefCell<HashMap<&'static str, wgpu::Buffer>>,
}

impl ScratchBuffers {
    /// Each role has one allocation. Callers finish and unmap a submission
    /// before reusing it; queue writes remain ordered before the next dispatch.
    pub fn get(
        &self,
        device: &wgpu::Device,
        role: &'static str,
        size: u64,
        usage: wgpu::BufferUsages,
    ) -> wgpu::Buffer {
        let mut buffers = self.buffers.borrow_mut();
        let limit = if usage.contains(wgpu::BufferUsages::STORAGE) {
            device
                .limits()
                .max_buffer_size
                .min(u64::from(device.limits().max_storage_buffer_binding_size))
        } else {
            device.limits().max_buffer_size
        };
        let capacity = size.max(4).next_power_of_two().min(limit);
        let buffer = buffers.entry(role).or_insert_with(|| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(role),
                size: capacity,
                usage,
                mapped_at_creation: false,
            })
        });
        assert_eq!(buffer.usage(), usage, "scratch buffer role changed usage");
        if buffer.size() < size {
            *buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(role),
                size: capacity,
                usage,
                mapped_at_creation: false,
            });
        }
        buffer.clone()
    }

    pub fn upload(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        role: &'static str,
        bytes: &[u8],
        usage: wgpu::BufferUsages,
    ) -> wgpu::Buffer {
        let buffer = self.get(
            device,
            role,
            bytes.len() as u64,
            usage | wgpu::BufferUsages::COPY_DST,
        );
        queue.write_buffer(&buffer, 0, bytes);
        buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, time::Duration};

    #[test]
    fn readback_wait_preserves_short_metal_completion_and_real_errors() {
        let elapsed = Cell::new(Duration::ZERO);
        let mut calls = 0;
        let result = wait_with_budget(
            true,
            || elapsed.get(),
            |timeout| {
                assert_eq!(timeout, METAL_SHORT_WAIT);
                calls += 1;
                elapsed.set(elapsed.get() + timeout);
                if calls == 1 {
                    Err(wgpu::PollError::Timeout)
                } else {
                    Ok(wgpu::PollStatus::WaitSucceeded)
                }
            },
        );
        assert!(result.unwrap().wait_finished());
        assert_eq!(calls, 2);
        let error = wait_with_budget(
            true,
            || Duration::ZERO,
            |_| Err(wgpu::PollError::WrongSubmissionIndex(1, 0)),
        );
        assert!(matches!(
            error,
            Err(wgpu::PollError::WrongSubmissionIndex(1, 0))
        ));
    }

    #[test]
    fn readback_wait_exhausts_one_budget_instead_of_falling_through_to_infinity() {
        let elapsed = Cell::new(Duration::ZERO);
        let mut waits = Vec::new();
        let error = wait_with_budget(
            true,
            || elapsed.get(),
            |timeout| {
                waits.push(timeout);
                // Model slow wakeups during the initial short waits.
                elapsed.set(elapsed.get() + timeout.max(Duration::from_millis(1)));
                Err(wgpu::PollError::Timeout)
            },
        );
        assert!(matches!(error, Err(wgpu::PollError::Timeout)));
        assert_eq!(&waits[..4], &[METAL_SHORT_WAIT; 4]);
        assert_eq!(waits.len(), 5);
        assert_eq!(waits[4], READBACK_BUDGET - Duration::from_millis(4));
        assert_eq!(elapsed.get(), READBACK_BUDGET);
    }

    #[test]
    fn readback_wait_is_bounded_on_other_backends_and_after_a_late_wakeup() {
        let error = wait_with_budget(
            false,
            || Duration::ZERO,
            |timeout| {
                assert_eq!(timeout, READBACK_BUDGET);
                Err(wgpu::PollError::Timeout)
            },
        );
        assert!(matches!(error, Err(wgpu::PollError::Timeout)));
        let error = wait_with_budget(
            true,
            || READBACK_BUDGET + Duration::from_secs(1),
            |timeout| {
                assert_eq!(timeout, Duration::ZERO);
                Err(wgpu::PollError::Timeout)
            },
        );
        assert!(matches!(error, Err(wgpu::PollError::Timeout)));
    }
}
