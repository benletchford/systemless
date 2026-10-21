//! Shared wgpu kernels for guest-visible QuickDraw and QD3D results.
//!
//! Each operation completes before its result is returned, so callers commit
//! pixels to guest memory exactly as they would from the CPU path. Hosts only
//! choose how to wait: the desktop blocks on a bounded device poll, while a
//! browser compute worker awaits WebGPU queue completion on its event loop.
//!
//! Dispatch heuristics are always available; the kernels require the
//! `wgpu-compute` feature.
#[cfg(feature = "wgpu-compute")]
mod buffers;
#[cfg(feature = "wgpu-compute")]
mod qd3d;
#[cfg(feature = "wgpu-compute")]
mod quickdraw;
mod routing;

#[cfg(feature = "wgpu-compute")]
#[doc(hidden)]
pub use qd3d::{raster_tiles, TRIANGLE_BYTES};
pub use routing::{
    qd3d_triangle_area, quickdraw_prefers_blit, quickdraw_prefers_raster, Qd3dWorkEstimate,
};
#[cfg(feature = "wgpu-compute")]
pub use {
    buffers::{little_endian_word_bytes, word_bytes, BindingRole, ScratchBuffers},
    qd3d::Qd3dCompute,
    quickdraw::QuickDrawCompute,
};

/// Host-owned device health. A recorded failure is sticky: the owner retires
/// the compute state and keeps guest drawing on the CPU.
#[cfg(feature = "wgpu-compute")]
pub trait ComputeHealth: Send + Sync {
    fn check(&self) -> Result<(), String>;
    /// Record a non-recoverable device or validation failure.
    fn record(&self, error: String);
    /// Make submitted map callbacks observable. Blocking hosts poll the device
    /// within a finite budget; asynchronous hosts return immediately and the
    /// readback future completes from their event loop.
    fn wait_readback(&self, device: &wgpu::Device) -> Result<(), String>;
}

#[cfg(feature = "wgpu-compute")]
struct Scopes([wgpu::ErrorScopeGuard; 3]);

#[cfg(feature = "wgpu-compute")]
impl Scopes {
    fn push(device: &wgpu::Device) -> Self {
        let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
        let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
        Self([validation, memory, internal])
    }

    /// Pop in reverse push order, recording each captured error.
    async fn pop(self, health: &dyn ComputeHealth) -> Option<String> {
        let mut error = None;
        for scope in self.0 {
            if let Some(e) = scope.pop().await {
                health.record(e.to_string());
                error = Some(e.to_string());
            }
        }
        error
    }

    /// Pipeline creation reports its own error without poisoning the device.
    async fn pop_unrecorded(self) -> Option<String> {
        let mut error = None;
        for scope in self.0 {
            if let Some(e) = scope.pop().await {
                error = Some(e.to_string());
            }
        }
        error
    }
}

#[cfg(feature = "wgpu-compute")]
/// Map the first `bytes` of a submitted readback buffer, convert them, and
/// unmap before the scratch role can be reused by another request.
async fn read_mapped<T>(
    health: &dyn ComputeHealth,
    device: &wgpu::Device,
    readback: &wgpu::Buffer,
    bytes: u64,
    convert: impl FnOnce(&[u8]) -> T,
) -> Result<T, String> {
    let (tx, rx) = futures_channel::oneshot::channel();
    readback
        .slice(..bytes)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
    health.wait_readback(device)?;
    rx.await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let mapped = readback
        .slice(..bytes)
        .get_mapped_range()
        .map_err(|e| e.to_string())?;
    let result = convert(&mapped);
    drop(mapped);
    readback.unmap();
    Ok(result)
}
