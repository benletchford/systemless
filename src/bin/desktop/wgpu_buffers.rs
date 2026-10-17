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
