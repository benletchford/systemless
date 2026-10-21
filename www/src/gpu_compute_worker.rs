//! WebGPU owner for guest-visible QuickDraw and QD3D compute.
//!
//! Runs in a dedicated worker whose event loop stays free while the
//! emulation worker blocks, so WebGPU map callbacks can complete. Requests are
//! executed one at a time: scratch buffers are reused between operations.
use crate::gpu_wire::{encode_response, Failure, Request, Response};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use systemless::gpu_compute::{ComputeHealth, Qd3dCompute, QuickDrawCompute};
use wasm_bindgen::prelude::*;

/// The first failure is sticky; later work is declined for CPU fallback.
#[derive(Default)]
struct BrowserHealth(Mutex<Option<String>>);

impl ComputeHealth for BrowserHealth {
    fn check(&self) -> Result<(), String> {
        match &*self.0.lock().unwrap_or_else(|e| e.into_inner()) {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }

    fn record(&self, error: String) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_or_insert(error);
    }

    /// Completion arrives on this worker's event loop while the readback
    /// future is pending; the emulation worker owns the deadline.
    fn wait_readback(&self, _device: &wgpu::Device) -> Result<(), String> {
        Ok(())
    }
}

struct Devices {
    health: Arc<BrowserHealth>,
    quickdraw: QuickDrawCompute,
    qd3d: Qd3dCompute,
}

#[wasm_bindgen]
pub struct GpuComputeWorker {
    devices: Rc<Devices>,
    adapter: String,
}

fn error(value: impl std::fmt::Display) -> JsValue {
    js_sys::Error::new(&value.to_string()).into()
}

#[wasm_bindgen]
impl GpuComputeWorker {
    pub async fn create() -> Result<GpuComputeWorker, JsValue> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            })
            .await
            .map_err(error)?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(error)?;
        let health = Arc::new(BrowserHealth::default());
        let lost = health.clone();
        device.set_device_lost_callback(move |reason, message| {
            lost.record(format!("WebGPU device lost: {reason:?}: {message}"));
        });
        let invalid = health.clone();
        device.on_uncaptured_error(Arc::new(move |cause| {
            invalid.record(format!("WebGPU error: {cause}"));
        }));
        let quickdraw =
            QuickDrawCompute::new(device.clone(), queue.clone(), health.clone()).map_err(error)?;
        let qd3d = Qd3dCompute::new(device, queue, health.clone())
            .await
            .map_err(error)?;
        let info = adapter.get_info();
        // Browsers may redact the adapter description.
        let adapter = [info.name.as_str(), info.driver.as_str()]
            .into_iter()
            .find(|label| !label.is_empty())
            .unwrap_or("WebGPU adapter")
            .to_owned();
        Ok(Self {
            devices: Rc::new(Devices {
                health,
                quickdraw,
                qd3d,
            }),
            adapter,
        })
    }

    pub fn adapter(&self) -> String {
        self.adapter.clone()
    }

    /// Resolve to an encoded reply. The promise never rejects: failures are
    /// encoded so the guest falls back to CPU rendering for that operation.
    pub fn execute(&self, request: Vec<u8>) -> js_sys::Promise {
        let devices = self.devices.clone();
        wasm_bindgen_futures::future_to_promise(async move {
            let reply = encode_response(&execute(&devices, &request).await);
            Ok(js_sys::Uint8Array::from(reply.as_slice()).into())
        })
    }

    /// Test-only device-loss injection; later requests report a fatal failure.
    #[wasm_bindgen(js_name = injectFailure)]
    pub fn inject_failure(&self, message: String) {
        self.devices.health.record(message);
    }
}

async fn execute(devices: &Devices, request: &[u8]) -> Result<Response, Failure> {
    let request = Request::decode(request).map_err(|error| Failure {
        fatal: true,
        message: error.to_string(),
    })?;
    if let Err(message) = devices.health.check() {
        return Err(Failure {
            fatal: true,
            message,
        });
    }
    let result = match request {
        Request::Raster {
            command,
            background,
        } => devices
            .quickdraw
            .render(&command, &background)
            .await
            .map(Response::Pixels),
        Request::BlitCells(command) => devices
            .quickdraw
            .render_blit_cells(&command)
            .await
            .map(Response::Pixels),
        Request::Blit(command) => devices
            .quickdraw
            .render_blit(&command)
            .await
            .map(Response::Words),
        Request::PackedBlit(command) => devices
            .quickdraw
            .render_packed_blit(&command)
            .await
            .map(Response::Bytes),
        Request::Qd3d {
            frame,
            background,
            clut,
        } => devices
            .qd3d
            .render_with_palette(&frame, &background, clut.as_deref())
            .await
            .map(Response::Halfwords),
    };
    result.map_err(|message| Failure {
        fatal: devices.health.check().is_err(),
        message,
    })
}
