//! Sparse asynchronous GPU timestamps. This measures the render pass, not compositor scanout.
use std::sync::mpsc;
pub(super) struct GpuProbe {
    pub queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    pending: Option<(u64, mpsc::Receiver<bool>)>,
    period: f64,
    pub samples: Vec<(u64, f64)>,
}
impl GpuProbe {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self {
            queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("render timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: 2,
            }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 256,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            read: device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 16,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
            pending: None,
            period: f64::from(queue.get_timestamp_period()),
            samples: Vec::new(),
        }
    }
    pub fn collect(&mut self) {
        let Some((sequence, rx)) = &self.pending else {
            return;
        };
        let Ok(success) = rx.try_recv() else {
            return;
        };
        if success {
            let data = self.read.slice(..).get_mapped_range();
            let start = u64::from_ne_bytes(data[0..8].try_into().unwrap());
            let end = u64::from_ne_bytes(data[8..16].try_into().unwrap());
            if self.samples.len() < 10_000 {
                self.samples.push((
                    *sequence,
                    end.saturating_sub(start) as f64 * self.period / 1e6,
                ));
            }
            drop(data);
            self.read.unmap();
        }
        self.pending = None;
    }
    pub fn available(&self) -> bool {
        self.pending.is_none()
    }
    pub fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.resolve_query_set(&self.queries, 0..2, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.read, 0, 16);
    }
    pub fn submitted(&mut self, sequence: u64) {
        let (tx, rx) = mpsc::channel();
        self.read
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result.is_ok());
            });
        self.pending = Some((sequence, rx));
    }
}
