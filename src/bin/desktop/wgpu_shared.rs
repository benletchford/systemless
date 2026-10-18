//! Fenced Metal UMA uploads. Rendering and ownership still belong to wgpu.
use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, Ordering},
};
use systemless::memory::CompactPresentation;

const SLOTS: usize = 3;
const MAX_PACKET: u64 = 32 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Layout {
    offsets: [u64; 3],
    sizes: [u64; 3],
    total: u64,
}
impl Layout {
    fn new(image: &CompactPresentation, limits: &wgpu::Limits) -> Option<Self> {
        let alignment = u64::from(
            limits
                .min_storage_buffer_offset_alignment
                .max(limits.min_uniform_buffer_offset_alignment),
        );
        let sizes = [
            image.cells.len().max(1) as u64 * 4,
            image.detail.len().max(1) as u64 * 4,
            64,
        ];
        if sizes[..2]
            .iter()
            .any(|&size| size > u64::from(limits.max_storage_buffer_binding_size))
        {
            return None;
        }
        let detail = sizes[0].div_ceil(alignment) * alignment;
        let geometry = (detail + sizes[1]).div_ceil(alignment) * alignment;
        let total = geometry + sizes[2];
        (total <= limits.max_buffer_size.min(MAX_PACKET)).then_some(Self {
            offsets: [0, detail, geometry],
            sizes,
            total,
        })
    }
}
struct Slot {
    buffer: wgpu::Buffer,
    group: wgpu::BindGroup,
    layout: Layout,
    ready: Arc<AtomicBool>,
    last_use: Option<wgpu::SubmissionIndex>,
    image: Weak<CompactPresentation>,
}
pub(super) struct Pool {
    device: wgpu::Device,
    queue: wgpu::Queue,
    slots: Vec<Slot>,
}
struct Upload {
    index: usize,
    group: wgpu::BindGroup,
    ready: Arc<AtomicBool>,
    submitted: bool,
}
impl Upload {
    fn submit(
        mut self,
        queue: &wgpu::Queue,
        encoder: wgpu::CommandEncoder,
    ) -> wgpu::SubmissionIndex {
        let commands = encoder.finish();
        // If submission panics after partially reaching the driver, fail closed:
        // this reservation stays unavailable until the pool is destroyed.
        self.submitted = true;
        queue.submit([commands])
    }
}
impl Drop for Upload {
    fn drop(&mut self) {
        if !self.submitted {
            self.ready.store(true, Ordering::Release);
        }
    }
}
impl Pool {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        if !device
            .features()
            .contains(wgpu::Features::MAPPABLE_PRIMARY_BUFFERS)
        {
            return None;
        }
        // Only Metal's host-visible shared memory is used here. The guard is
        // dropped immediately; no raw resource is retained or destroyed.
        unsafe { device.as_hal::<wgpu::hal::api::Metal>() }?;
        Some(Self {
            device: device.clone(),
            queue: queue.clone(),
            slots: Vec::new(),
        })
    }
    fn completed(&self, slot: &Slot) -> bool {
        slot.ready.load(Ordering::Acquire)
            || slot.last_use.as_ref().is_some_and(|index| {
                self.device
                    .poll(wgpu::PollType::Wait {
                        submission_index: Some(index.clone()),
                        timeout: Some(std::time::Duration::ZERO),
                    })
                    .is_ok_and(|status| status.wait_finished())
            })
    }
    #[cfg(test)]
    pub fn status(&self) -> (usize, usize) {
        (
            self.slots.len(),
            self.slots
                .iter()
                .filter(|slot| self.completed(slot))
                .count(),
        )
    }
    pub fn supports(&self, image: &CompactPresentation, limits: &wgpu::Limits) -> bool {
        Layout::new(image, limits).is_some()
    }
    pub fn render(
        &mut self,
        binding: &wgpu::BindGroupLayout,
        image: &Arc<CompactPresentation>,
        geometry: &[u32; 16],
        pipeline: &wgpu::RenderPipeline,
        target: &wgpu::TextureView,
        viewport: (u32, u32, u32, u32),
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) -> bool {
        let Some(upload) = self.upload(binding, image, geometry) else {
            return false;
        };
        let mut encoder = self.device.create_command_encoder(&Default::default());
        super::wgpu_compact::draw(
            &mut encoder,
            pipeline,
            &upload.group,
            target,
            viewport,
            timestamps,
        );
        let index = upload.index;
        let submission = upload.submit(&self.queue, encoder);
        self.slots[index].last_use = Some(submission);
        true
    }
    fn upload(
        &mut self,
        binding: &wgpu::BindGroupLayout,
        image: &Arc<CompactPresentation>,
        geometry: &[u32; 16],
    ) -> Option<Upload> {
        let device = &self.device;
        let limits = device.limits();
        let layout = Layout::new(image, &limits)?;
        // Checking individual submission fences also reclaims completed work;
        // a separate device poll would duplicate that work on every frame.
        let index = self
            .slots
            .iter()
            .position(|slot| self.completed(slot))
            .or_else(|| (self.slots.len() < SLOTS).then_some(self.slots.len()))?;
        let group = |buffer: &wgpu::Buffer| {
            let entries: Vec<_> = (0..3)
                .map(|i| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer,
                        offset: layout.offsets[i],
                        size: wgpu::BufferSize::new(layout.sizes[i]),
                    }),
                })
                .collect();
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("fenced shared retained packet"),
                layout: binding,
                entries: &entries,
            })
        };
        if self
            .slots
            .get(index)
            .is_none_or(|slot| slot.buffer.size() < layout.total)
        {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("fenced shared retained packet"),
                size: layout
                    .total
                    .next_power_of_two()
                    .min(limits.max_buffer_size.min(MAX_PACKET)),
                // Metal MAP_WRITE chooses write-combined memory. MAP_READ
                // selects CPU-cached shared memory, matching native presentation.
                // HAL mapping permits CPU writes with either mappable usage;
                // no frontend read/write mapping is exposed by this pool.
                usage: wgpu::BufferUsages::MAP_READ
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::UNIFORM,
                mapped_at_creation: false,
            });
            // Initialize through wgpu, including its initialization tracker.
            // Never race the GPU clear with the first HAL write: reserve this
            // slot behind its initialization fence and fall back for this frame.
            let mut init = device.create_command_encoder(&Default::default());
            init.clear_buffer(&buffer, 0, None);
            let initial_use = self.queue.submit([init.finish()]);
            let slot = Slot {
                group: group(&buffer),
                buffer,
                layout,
                ready: Arc::new(AtomicBool::new(false)),
                last_use: Some(initial_use),
                image: Weak::new(),
            };
            if index == self.slots.len() {
                self.slots.push(slot);
            } else {
                self.slots[index] = slot;
            }
            return None;
        }
        let slot = &mut self.slots[index];
        if slot.layout != layout {
            slot.group = group(&slot.buffer);
            slot.layout = layout;
        }
        slot.ready.store(false, Ordering::Release);
        slot.last_use = None;
        // A Weak keeps the control block allocated, preventing pointer reuse
        // from making a new packet look like an old immutable packet.
        let same_image = std::ptr::eq(slot.image.as_ptr(), Arc::as_ptr(image));
        if !write(
            device,
            &slot.buffer,
            layout,
            [&image.cells, &image.detail, geometry],
            same_image,
        ) {
            slot.ready.store(true, Ordering::Release);
            return None;
        }
        if !same_image {
            slot.image = Arc::downgrade(image);
        }
        Some(Upload {
            index,
            group: slot.group.clone(),
            ready: slot.ready.clone(),
            submitted: false,
        })
    }
}
fn write(
    device: &wgpu::Device,
    buffer: &wgpu::Buffer,
    layout: Layout,
    words: [&[u32]; 3],
    same_image: bool,
) -> bool {
    use wgpu::hal::Device as _;
    let size = buffer.size();
    // SAFETY: these private Metal buffers have MAP_READ usage. HAL's mapping
    // contract permits CPU writes to either MAP_READ or MAP_WRITE allocations;
    // MAP_READ avoids Metal's uncached/write-combined upload allocation.
    // Each slot is unused or its previous submission has completed, confirmed
    // by a successful zero-timeout fence poll. A reserved upload cannot be
    // reused before submit or cancellation. No GPU access overlaps writes. HAL
    // explicitly permits persistent mappings; wgpu's own map state remains
    // unmapped. All offsets and lengths were bounded by Layout and capacity.
    // Resource guards keep device/buffer alive, and no pointer escapes unmap.
    unsafe {
        let Some(device) = device.as_hal::<wgpu::hal::api::Metal>() else {
            return false;
        };
        let Some(buffer) = buffer.as_hal::<wgpu::hal::api::Metal>() else {
            return false;
        };
        let Ok(mapping) = device.map_buffer(&buffer, 0..size) else {
            return false;
        };
        for (i, data) in words.into_iter().enumerate() {
            if same_image && i < 2 {
                continue;
            }
            let bytes = super::wgpu_buffers::word_bytes(data);
            debug_assert!(bytes.len() as u64 <= layout.sizes[i]);
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                mapping.ptr.as_ptr().add(layout.offsets[i] as usize),
                bytes.len(),
            );
        }
        if !mapping.is_coherent {
            device.flush_mapped_ranges(&buffer, std::iter::once(0..size));
        }
        device.unmap_buffer(&buffer);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gpu_shared_slots_are_bounded_and_cancelled_uploads_reusable() {
        let instance = wgpu::Instance::default();
        let adapter = match pollster::block_on(instance.request_adapter(&Default::default())) {
            Ok(adapter) => adapter,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        let (device, queue) = super::super::wgpu_present::request_device(&adapter).unwrap();
        let Some(mut pool) = Pool::new(&device, &queue) else {
            return;
        };
        let entries: Vec<_> = (0..3)
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: if binding == 2 {
                        wgpu::BufferBindingType::Uniform
                    } else {
                        wgpu::BufferBindingType::Storage { read_only: true }
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect();
        let binding = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &entries,
        });
        let image = Arc::new(CompactPresentation {
            width: 1,
            height: 1,
            scale: 1,
            cells: vec![0x123456],
            detail: Vec::new(),
        });
        let geometry = [1, 1, 1, 0, 0, 0, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1];
        let mut uploads = Vec::new();
        for _ in 0..3 {
            let upload = match pool.upload(&binding, &image, &geometry) {
                Some(upload) => upload,
                None => {
                    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                    pool.upload(&binding, &image, &geometry).unwrap()
                }
            };
            uploads.push(upload);
        }
        assert_eq!(pool.status(), (3, 0));
        assert!(pool.upload(&binding, &image, &geometry).is_none());
        drop(uploads.pop());
        let upload = pool.upload(&binding, &image, &geometry).unwrap();
        assert_eq!(pool.status(), (3, 0));
        drop(upload);
        drop(uploads);
        assert_eq!(pool.status(), (3, 3));
        let mut limits = device.limits();
        limits.max_buffer_size = 16;
        assert!(!pool.supports(&image, &limits));
    }
}
