//! Reusable resources for guest-visible GPU operations.
use std::cell::RefCell;
use std::collections::HashMap;

/// Borrow packed native u32 storage without allocating or serializing each
/// word. This is the same byte order as u32::to_ne_bytes used by presentation.
pub fn word_bytes(words: &[u32]) -> &[u8] {
    // SAFETY: u32 has no padding and every byte is initialized. u8 accepts any
    // alignment and bit pattern. The returned borrow cannot outlive `words`.
    unsafe { std::slice::from_raw_parts(words.as_ptr().cast(), std::mem::size_of_val(words)) }
}

pub fn little_endian_word_bytes(words: &[u32]) -> std::borrow::Cow<'_, [u8]> {
    #[cfg(target_endian = "little")]
    {
        std::borrow::Cow::Borrowed(word_bytes(words))
    }
    #[cfg(target_endian = "big")]
    {
        std::borrow::Cow::Owned(words.iter().flat_map(|word| word.to_le_bytes()).collect())
    }
}

#[derive(Clone, Copy)]
pub enum BindingRole {
    Raster,
    PolygonProducts,
    Bitmap,
    Qd3d,
    Qd3dIndexed,
}

struct CachedBindings {
    pipeline: wgpu::ComputePipeline,
    buffers: Vec<(u32, wgpu::Buffer)>,
    group: wgpu::BindGroup,
}

#[derive(Default)]
pub struct ScratchBuffers {
    buffers: RefCell<HashMap<&'static str, wgpu::Buffer>>,
    // Four fixed roles plus the five bitmap pipeline variants. No history
    // grows with request count, and scratch growth invalidates every slot.
    bindings: RefCell<[Option<CachedBindings>; 9]>,
    #[cfg(feature = "test-support")]
    binding_creations: std::cell::Cell<u64>,
}

impl ScratchBuffers {
    #[cfg(feature = "test-support")]
    pub fn resource_snapshot(&self) -> serde_json::Value {
        let buffers = self.buffers.borrow();
        let capacities: std::collections::BTreeMap<_, _> = buffers
            .iter()
            .map(|(role, buffer)| (*role, buffer.size()))
            .collect();
        serde_json::json!({
            "buffers": capacities,
            "capacity_bytes": buffers.values().map(|b| b.size()).sum::<u64>(),
            "bind_group_cache_entries": self.bindings.borrow().iter().filter(|entry| entry.is_some()).count(),
            "bind_group_creations": self.binding_creations.get(),
            "scope": "requested scratch-buffer capacity; not driver residency",
        })
    }

    pub fn bind_group(
        &self,
        device: &wgpu::Device,
        role: BindingRole,
        pipeline: &wgpu::ComputePipeline,
        buffers: &[(u32, &wgpu::Buffer)],
    ) -> wgpu::BindGroup {
        let mut bindings = self.bindings.borrow_mut();
        let slot = match role {
            BindingRole::Raster => 0,
            BindingRole::PolygonProducts => 1,
            BindingRole::Qd3d => 2,
            BindingRole::Qd3dIndexed => 3,
            BindingRole::Bitmap => (4..9)
                .find(|&slot| {
                    bindings[slot]
                        .as_ref()
                        .is_some_and(|entry| entry.pipeline == *pipeline)
                })
                .or_else(|| (4..9).find(|&slot| bindings[slot].is_none()))
                // A future sixth variant replaces an entry instead of growing.
                .unwrap_or(4),
        };
        let cached = &mut bindings[slot];
        if let Some(entry) = cached {
            if entry.pipeline == *pipeline
                && entry.buffers.len() == buffers.len()
                && entry.buffers.iter().zip(buffers).all(
                    |((slot, buffer), (new_slot, new_buffer))| {
                        slot == new_slot && buffer == *new_buffer
                    },
                )
            {
                return entry.group.clone();
            }
        }
        let entries: Vec<_> = buffers
            .iter()
            .map(|(binding, buffer)| wgpu::BindGroupEntry {
                binding: *binding,
                resource: buffer.as_entire_binding(),
            })
            .collect();
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cached compute scratch bindings"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        });
        *cached = Some(CachedBindings {
            pipeline: pipeline.clone(),
            buffers: buffers
                .iter()
                .map(|(slot, buffer)| (*slot, (*buffer).clone()))
                .collect(),
            group: group.clone(),
        });
        #[cfg(feature = "test-support")]
        self.binding_creations.set(self.binding_creations.get() + 1);
        group
    }

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
            // No cached group may retain a previous scratch allocation after
            // growth, including groups from other pipelines sharing that role.
            self.bindings
                .borrow_mut()
                .iter_mut()
                .for_each(|entry| *entry = None);
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
