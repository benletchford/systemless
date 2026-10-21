//! Guest-visible GPU rendering from the emulation worker.
//!
//! The renderer traits are synchronous: guest execution resumes only after
//! results are committed to guest memory. A browser cannot wait for WebGPU on
//! the thread that owns the device, so the emulation worker sends a snapshot
//! to a dedicated compute worker and blocks on a shared reply buffer. Any
//! failure returns an error, which leaves guest pixels to the CPU path.
use crate::gpu_wire::{decode_response, Request, RequestKind, Response};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use systemless::gpu_compute::{quickdraw_prefers_blit, quickdraw_prefers_raster, Qd3dWorkEstimate};
use systemless::loader::ppc::{PpcQ3GpuFrame, PpcQ3GpuRenderer, PpcQ3RasterFrame};
use systemless::quickdraw::gpu::{
    QuickDrawGpuBlit, QuickDrawGpuCommand, QuickDrawGpuPackedCopy, QuickDrawGpuPixel,
    QuickDrawGpuRenderer,
};

/// Blocking request/reply channel to the device owner.
pub trait GpuTransport {
    /// `capacity` bounds the encoded reply. An error means the reply is
    /// unavailable, including a timeout; the transport must then reject all
    /// later calls, because a late reply could otherwise be misattributed.
    fn call(&self, request: &[u8], capacity: usize) -> Result<Vec<u8>, String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dispatch {
    /// No requests; guest drawing stays on the CPU.
    Off,
    /// The desktop heuristics: only work that amortizes a round trip.
    Adaptive,
    /// Every supported operation, for CPU/GPU equivalence checks.
    Forced,
}

impl Dispatch {
    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Adaptive => "adaptive",
            Self::Forced => "force",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "off" | "cpu" | "0" => Some(Self::Off),
            "adaptive" | "gpu" | "1" => Some(Self::Adaptive),
            "force" | "forced" => Some(Self::Forced),
            _ => None,
        }
    }
}

/// Page options for guest-visible GPU compute. `?renderer=wgpu` enables
/// adaptive dispatch; `gpu_compute=adaptive|force|off` selects it explicitly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputeOptions {
    pub mode: &'static str,
    pub timeout_ms: Option<u32>,
    /// Test-only: fail the device this long after it becomes ready.
    pub inject_failure_after_ms: Option<u32>,
}

impl ComputeOptions {
    pub fn from_query(search: &str) -> Option<Self> {
        let mut renderer_wgpu = false;
        let mut mode = None;
        let mut timeout_ms = None;
        let mut inject_failure_after_ms = None;
        for part in search.trim_start_matches('?').split('&') {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            match key {
                "renderer" => renderer_wgpu = value == "wgpu",
                "gpu_compute" => mode = Some(Dispatch::parse(value)),
                "gpu_compute_timeout_ms" => timeout_ms = value.parse().ok(),
                "gpu_compute_inject_loss_ms" => inject_failure_after_ms = value.parse().ok(),
                _ => {}
            }
        }
        let dispatch = match mode {
            Some(Some(dispatch)) => dispatch,
            // An unrecognized explicit mode never enables GPU work.
            Some(None) => return None,
            None if renderer_wgpu => Dispatch::Adaptive,
            None => return None,
        };
        (dispatch != Dispatch::Off).then_some(Self {
            mode: dispatch.name(),
            timeout_ms: timeout_ms.filter(|ms| (100..=60_000).contains(ms)),
            inject_failure_after_ms,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KindStats {
    pub completed: u64,
    pub failed: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProxyStats {
    pub raster: KindStats,
    pub blit: KindStats,
    pub qd3d: KindStats,
    /// Sticky transport or device failure; all later work stays on the CPU.
    pub disabled: Option<String>,
    pub last_error: Option<String>,
}

struct Shared {
    transport: Box<dyn GpuTransport>,
    dispatch: Cell<Dispatch>,
    stats: RefCell<ProxyStats>,
}

/// Cloneable handle: one copy is installed for QuickDraw, one for QD3D, and
/// the worker runtime keeps one for dispatch changes and diagnostics.
#[derive(Clone)]
pub struct GpuProxy(Rc<Shared>);

impl GpuProxy {
    pub fn new(transport: Box<dyn GpuTransport>, dispatch: Dispatch) -> Self {
        Self(Rc::new(Shared {
            transport,
            dispatch: Cell::new(dispatch),
            stats: Default::default(),
        }))
    }

    pub fn set_dispatch(&self, dispatch: Dispatch) {
        self.0.dispatch.set(dispatch);
    }

    pub fn stats(&self) -> ProxyStats {
        self.0.stats.borrow().clone()
    }

    fn dispatch(&self) -> Dispatch {
        if self.0.stats.borrow().disabled.is_some() {
            Dispatch::Off
        } else {
            self.0.dispatch.get()
        }
    }

    /// One blocking round trip. `accept` checks the reply shape before the
    /// operation is counted as completed and its pixels reach guest memory.
    fn call<T>(
        &self,
        request: Request,
        accept: impl FnOnce(Response) -> Option<T>,
    ) -> Result<T, String> {
        if let Some(reason) = &self.0.stats.borrow().disabled {
            return Err(reason.clone());
        }
        let kind = request.kind();
        let reply = self
            .0
            .transport
            .call(&request.encode(), request.response_capacity());
        let mut stats = self.0.stats.borrow_mut();
        let result = match reply.map(|bytes| decode_response(&bytes)) {
            Ok(Ok(response)) => {
                accept(response).ok_or_else(|| "GPU compute returned an unexpected response".into())
            }
            Ok(Err(failure)) => {
                if failure.fatal {
                    stats.disabled = Some(failure.message.clone());
                }
                Err(failure.message)
            }
            Err(error) => {
                let error = format!("GPU compute unavailable: {error}");
                stats.disabled = Some(error.clone());
                Err(error)
            }
        };
        let counter = match kind {
            RequestKind::Raster => &mut stats.raster,
            RequestKind::Blit => &mut stats.blit,
            RequestKind::Qd3d => &mut stats.qd3d,
        };
        match &result {
            Ok(_) => counter.completed += 1,
            Err(error) => {
                counter.failed += 1;
                if stats.last_error.as_ref() != Some(error) {
                    web_log(&format!("[GPU] CPU fallback: {error}"));
                    stats.last_error = Some(error.clone());
                }
            }
        }
        result
    }
}

fn web_log(message: &str) {
    #[cfg(target_arch = "wasm32")]
    web_sys::console::warn_1(&message.into());
    #[cfg(not(target_arch = "wasm32"))]
    let _ = message;
}

impl QuickDrawGpuRenderer for GpuProxy {
    fn prefers_precomputed_raster(&self) -> bool {
        // Adaptive dispatch evaluates polygon coverage in the kernel. Forced
        // dispatch also sends CPU-built spans and glyph atlases, exercising
        // every raster shape, as the desktop forced writeback tests do.
        self.dispatch() == Dispatch::Forced
    }

    fn prefers_raster(&self, command: &QuickDrawGpuCommand) -> bool {
        match self.dispatch() {
            Dispatch::Off => false,
            Dispatch::Adaptive => quickdraw_prefers_raster(command),
            Dispatch::Forced => true,
        }
    }

    fn prefers_blit(&self, destination_pixels: usize, expensive_color: bool) -> bool {
        match self.dispatch() {
            Dispatch::Off => false,
            Dispatch::Adaptive => quickdraw_prefers_blit(destination_pixels, expensive_color),
            Dispatch::Forced => true,
        }
    }

    fn render_blit_cells(
        &mut self,
        command: &QuickDrawGpuBlit,
    ) -> Result<Vec<QuickDrawGpuPixel>, String> {
        self.call(Request::BlitCells(command.clone()), |reply| match reply {
            Response::Pixels(pixels) => Some(pixels),
            _ => None,
        })
    }

    fn render_packed_blit(&mut self, command: &QuickDrawGpuPackedCopy) -> Result<Vec<u8>, String> {
        self.call(Request::PackedBlit(command.clone()), |reply| match reply {
            Response::Bytes(bytes) => Some(bytes),
            _ => None,
        })
    }

    fn render_blit(&mut self, command: &QuickDrawGpuBlit) -> Result<Vec<u32>, String> {
        self.call(Request::Blit(command.clone()), |reply| match reply {
            Response::Words(words) => Some(words),
            _ => None,
        })
    }

    fn render(
        &mut self,
        command: &QuickDrawGpuCommand,
        background: &[u32],
    ) -> Result<Vec<QuickDrawGpuPixel>, String> {
        self.call(
            Request::Raster {
                command: command.clone(),
                background: background.to_vec(),
            },
            |reply| match reply {
                Response::Pixels(pixels) => Some(pixels),
                _ => None,
            },
        )
    }
}

impl GpuProxy {
    fn render_frame(
        &self,
        frame: &PpcQ3RasterFrame,
        background: &[u16],
        clut: Option<&[[u16; 3]; 256]>,
    ) -> Result<Vec<u16>, String> {
        self.call(
            Request::Qd3d {
                frame: frame.clone(),
                background: background.to_vec(),
                clut: clut.map(|clut| Box::new(*clut)),
            },
            |reply| match reply {
                Response::Halfwords(pixels) if pixels.len() == background.len() => Some(pixels),
                _ => None,
            },
        )
    }
}

impl PpcQ3GpuRenderer for GpuProxy {
    fn prefers_native(&self, frame: &PpcQ3RasterFrame) -> bool {
        match self.dispatch() {
            Dispatch::Off => false,
            Dispatch::Adaptive => Qd3dWorkEstimate::of(frame).prefers_gpu(),
            Dispatch::Forced => true,
        }
    }

    fn render(&mut self, frame: &PpcQ3GpuFrame, background: &[u16]) -> Result<Vec<u16>, String> {
        self.render_native(&PpcQ3RasterFrame::from(frame), background)
    }

    fn render_native(
        &mut self,
        frame: &PpcQ3RasterFrame,
        background: &[u16],
    ) -> Result<Vec<u16>, String> {
        self.render_frame(frame, background, None)
    }

    fn render_indexed(
        &mut self,
        frame: &PpcQ3RasterFrame,
        background: &[u16],
        clut: &[[u16; 3]; 256],
    ) -> Result<Vec<u16>, String> {
        self.render_frame(frame, background, Some(clut))
    }
}

/// The emulation worker's `systemlessGpuCompute(request, capacity)` global.
/// It returns the reply bytes, or throws once the compute worker is lost.
#[cfg(target_arch = "wasm32")]
pub struct WorkerTransport;

#[cfg(target_arch = "wasm32")]
impl GpuTransport for WorkerTransport {
    fn call(&self, request: &[u8], capacity: usize) -> Result<Vec<u8>, String> {
        use wasm_bindgen::JsCast;
        let function = js_sys::Reflect::get(
            &js_sys::global(),
            &wasm_bindgen::JsValue::from_str("systemlessGpuCompute"),
        )
        .ok()
        .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
        .ok_or("compute bridge is not installed")?;
        let reply = function
            .call2(
                &wasm_bindgen::JsValue::UNDEFINED,
                &js_sys::Uint8Array::from(request),
                &wasm_bindgen::JsValue::from_f64(capacity as f64),
            )
            .map_err(|error| {
                error
                    .dyn_ref::<js_sys::Error>()
                    .map(|error| String::from(error.message()))
                    .or_else(|| error.as_string())
                    .unwrap_or_else(|| "compute bridge failed".into())
            })?;
        reply
            .dyn_into::<js_sys::Uint8Array>()
            .map(|bytes| bytes.to_vec())
            .map_err(|_| "compute bridge returned no reply".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu_wire::{encode_response, Failure};

    /// Decodes each request and answers like a device that copies backgrounds.
    struct Loopback {
        calls: Rc<Cell<u32>>,
        fail_after: Option<u32>,
    }

    impl GpuTransport for Loopback {
        fn call(&self, request: &[u8], capacity: usize) -> Result<Vec<u8>, String> {
            self.calls.set(self.calls.get() + 1);
            if self
                .fail_after
                .is_some_and(|limit| self.calls.get() > limit)
            {
                return Err("timed out".into());
            }
            let reply = match Request::decode(request).map_err(|e| e.to_string())? {
                Request::Raster { background, .. } => Ok(Response::Pixels(
                    background
                        .into_iter()
                        .map(|value| QuickDrawGpuPixel {
                            value: !value,
                            covered: true,
                        })
                        .collect(),
                )),
                Request::Qd3d { background, .. } => Ok(Response::Halfwords(
                    background.into_iter().map(|v| v & 0x7fff).collect(),
                )),
                Request::Blit(_) => Ok(Response::Bytes(vec![1])),
                Request::PackedBlit(_) => Err(Failure {
                    fatal: true,
                    message: "device lost".into(),
                }),
                Request::BlitCells(_) => Err(Failure {
                    fatal: false,
                    message: "exceeds limits".into(),
                }),
            };
            let bytes = encode_response(&reply);
            assert!(bytes.len() <= capacity);
            Ok(bytes)
        }
    }

    fn command() -> QuickDrawGpuCommand {
        QuickDrawGpuCommand {
            mask: None,
            clip_rows: None,
            polygon_edges: Some(vec![[0., 2., 0., 0.]]),
            area: [0, 0, 2, 1],
            rect: [0, 0, 2, 1],
            border: None,
            pattern: [0xff; 8],
            transfer: [[0, 0], [0, 0]],
        }
    }

    fn frame() -> PpcQ3RasterFrame {
        PpcQ3RasterFrame {
            width: 2,
            height: 1,
            viewport: [0, 0, 1, 0],
            clear_color: None,
            textures: vec![],
            draws: vec![],
        }
    }

    fn proxy(fail_after: Option<u32>, dispatch: Dispatch) -> (GpuProxy, Rc<Cell<u32>>) {
        let calls = Rc::new(Cell::new(0));
        let transport = Loopback {
            calls: calls.clone(),
            fail_after,
        };
        (GpuProxy::new(Box::new(transport), dispatch), calls)
    }

    #[test]
    fn replies_reach_the_guest_and_are_counted_by_kind() {
        let (mut proxy, _) = proxy(None, Dispatch::Forced);
        let pixels = QuickDrawGpuRenderer::render(&mut proxy, &command(), &[1, 2]).unwrap();
        assert_eq!(pixels[0].value, !1);
        let frame_pixels = proxy.render_native(&frame(), &[0x8001, 2]).unwrap();
        assert_eq!(frame_pixels, vec![1, 2]);
        // A mismatched reply variant is an error, never committed pixels.
        assert!(QuickDrawGpuRenderer::render_blit(&mut proxy, &blit()).is_err());
        let stats = proxy.stats();
        assert_eq!(stats.raster.completed, 1);
        assert_eq!(stats.qd3d.completed, 1);
        assert_eq!(stats.blit.failed, 1);
        assert!(stats.disabled.is_none());
        // Declined work is per operation; a fatal device failure is sticky.
        assert!(proxy.render_blit_cells(&blit()).is_err());
        assert!(proxy.prefers_blit(1, true));
        assert!(proxy.render_packed_blit(&packed()).is_err());
        assert!(!proxy.prefers_blit(1, true));
        assert_eq!(proxy.stats().disabled.as_deref(), Some("device lost"));
    }

    fn packed() -> QuickDrawGpuPackedCopy {
        QuickDrawGpuPackedCopy {
            depth: 8,
            width: 1,
            height: 1,
            source_stride: 1,
            source_bit: 0,
            source: vec![0],
            destination_stride: 1,
            destination_bit: 0,
            background: vec![0],
            palette: None,
        }
    }

    #[test]
    fn transport_failure_is_sticky_and_returns_work_to_the_cpu() {
        let (mut proxy, calls) = proxy(Some(1), Dispatch::Forced);
        assert!(proxy.prefers_raster(&command()));
        assert!(QuickDrawGpuRenderer::render(&mut proxy, &command(), &[0, 0]).is_ok());
        assert!(QuickDrawGpuRenderer::render(&mut proxy, &command(), &[0, 0]).is_err());
        assert_eq!(calls.get(), 2);
        // No further round trips are attempted, and no work is preferred.
        assert!(QuickDrawGpuRenderer::render(&mut proxy, &command(), &[0, 0]).is_err());
        assert_eq!(calls.get(), 2);
        assert!(!proxy.prefers_raster(&command()));
        assert!(!proxy.prefers_blit(1 << 20, true));
        assert!(!proxy.prefers_native(&frame()));
        assert!(proxy.stats().disabled.unwrap().contains("timed out"));
    }

    #[test]
    fn dispatch_modes_select_work_before_any_request() {
        let (proxy, calls) = proxy(None, Dispatch::Off);
        assert!(!proxy.prefers_raster(&command()));
        assert!(!proxy.prefers_native(&frame()));
        proxy.set_dispatch(Dispatch::Adaptive);
        // A two-pixel polygon cannot amortize a round trip.
        assert!(!proxy.prefers_raster(&command()));
        assert!(proxy.prefers_blit(16_384, true));
        assert!(!proxy.prefers_blit(16_384, false));
        assert!(!proxy.prefers_precomputed_raster());
        proxy.set_dispatch(Dispatch::Forced);
        assert!(proxy.prefers_raster(&command()));
        assert!(proxy.prefers_precomputed_raster());
        assert_eq!(calls.get(), 0);
        assert_eq!(Dispatch::parse("force"), Some(Dispatch::Forced));
        assert_eq!(Dispatch::parse("cpu"), Some(Dispatch::Off));
        assert_eq!(Dispatch::parse("nope"), None);
    }

    #[test]
    fn page_options_require_an_explicit_opt_in() {
        assert_eq!(ComputeOptions::from_query(""), None);
        assert_eq!(ComputeOptions::from_query("?renderer=worker"), None);
        let wgpu = ComputeOptions::from_query("?renderer=wgpu").unwrap();
        assert_eq!(wgpu.mode, "adaptive");
        assert_eq!(
            ComputeOptions::from_query("?renderer=wgpu&gpu_compute=off"),
            None
        );
        assert_eq!(
            ComputeOptions::from_query("?gpu_compute=bogus&renderer=wgpu"),
            None
        );
        let forced =
            ComputeOptions::from_query("?gpu_compute=force&gpu_compute_timeout_ms=9000").unwrap();
        assert_eq!(
            forced,
            ComputeOptions {
                mode: "force",
                timeout_ms: Some(9000),
                inject_failure_after_ms: None,
            }
        );
        // Out-of-range deadlines keep the default rather than hanging the guest.
        let unbounded = ComputeOptions::from_query("?gpu_compute=1&gpu_compute_timeout_ms=0");
        assert_eq!(unbounded.unwrap().timeout_ms, None);
        let loss = ComputeOptions::from_query("?gpu_compute=force&gpu_compute_inject_loss_ms=50");
        assert_eq!(loss.unwrap().inject_failure_after_ms, Some(50));
    }

    fn blit() -> QuickDrawGpuBlit {
        QuickDrawGpuBlit {
            width: 1,
            height: 1,
            pixels: vec![0],
            columns: vec![[0, 1]],
            rows: vec![[0, 1]],
            palette: None,
            direct: None,
            clip: None,
            arithmetic: None,
            deep_mask: None,
        }
    }
}
