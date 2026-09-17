//! Windows compact-coverage presenter (disable with SYSTEMLESS_D3D11=0).
//! Device/context access stays on the window thread. A wait worker wakes it
//! when DXGI is ready, independently of the guest tick and input handling.
use systemless::memory::CompactPresentation;
#[path = "d3d_wait.rs"]
mod d3d_wait;
use d3d_wait::FrameWaiter;
use windows::{
    core::{s, w, Interface, PCSTR},
    Win32::{
        Foundation::{CloseHandle, HANDLE, HMODULE, HWND},
        Graphics::{Direct3D::Fxc::*, Direct3D::*, Direct3D11::*, Dxgi::Common::*, Dxgi::*},
        UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
            WINDOW_EX_STYLE, WS_CHILD, WS_DISABLED, WS_VISIBLE,
        },
    },
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

type Result<T> = std::result::Result<T, String>;
fn error(e: windows::core::Error) -> String {
    e.to_string()
}

/// A flip-model HWND cannot return to GDI, even after its swap chain is gone.
/// Keep that restriction on a disposable child so the parent remains usable
/// by softbuffer after any GPU failure. Disabled children pass mouse input to
/// their parent; winit continues to own input, focus and the native cursor.
struct PresentationWindow(HWND);

impl Drop for PresentationWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

struct Upload {
    buffer: ID3D11Buffer,
    view: ID3D11ShaderResourceView,
    capacity: usize,
}

impl Upload {
    unsafe fn new(device: &ID3D11Device, length: usize) -> Result<Self> {
        let capacity = length
            .max(4)
            .checked_next_power_of_two()
            .ok_or("buffer too large")?;
        let bytes = capacity
            .checked_mul(4)
            .filter(|&v| v <= 128 * 1024 * 1024)
            .ok_or("buffer too large")?;
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: bytes as u32,
            Usage: D3D11_USAGE_DYNAMIC,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
            ..Default::default()
        };
        let mut buffer = None;
        device
            .CreateBuffer(&desc, None, Some(&mut buffer))
            .map_err(error)?;
        let buffer = buffer.ok_or("missing upload buffer")?;
        let desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_R32_UINT,
            ViewDimension: D3D_SRV_DIMENSION_BUFFER,
            Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                Buffer: D3D11_BUFFER_SRV {
                    Anonymous1: D3D11_BUFFER_SRV_0 { FirstElement: 0 },
                    Anonymous2: D3D11_BUFFER_SRV_1 {
                        NumElements: capacity as u32,
                    },
                },
            },
        };
        let mut view = None;
        device
            .CreateShaderResourceView(&buffer, Some(&desc), Some(&mut view))
            .map_err(error)?;
        Ok(Self {
            buffer,
            view: view.ok_or("missing buffer view")?,
            capacity,
        })
    }

    unsafe fn write(&self, context: &ID3D11DeviceContext, data: &[u32]) -> Result<()> {
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        context
            .Map(
                &self.buffer,
                0,
                D3D11_MAP_WRITE_DISCARD,
                0,
                Some(&mut mapped),
            )
            .map_err(error)?;
        // Map succeeded; this buffer is at least data.len() words and Unmap
        // happens before its SRV is used by the draw.
        std::ptr::copy_nonoverlapping(data.as_ptr(), mapped.pData.cast(), data.len());
        context.Unmap(&self.buffer, 0);
        Ok(())
    }
}

pub struct D3dPresenter {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swap: IDXGISwapChain2,
    ready: HANDLE,
    waiter: Option<FrameWaiter>,
    queue_slot: bool,
    retry_at: Option<std::time::Instant>,
    target: Option<ID3D11RenderTargetView>,
    size: (u32, u32),
    vertex: ID3D11VertexShader,
    pixel: ID3D11PixelShader,
    raster: ID3D11RasterizerState,
    constants: ID3D11Buffer,
    cells: Option<Upload>,
    detail: Option<Upload>,
    verified_size: Option<(u32, u32)>,
    frames: u64,
    // Fields drop in declaration order: release all D3D objects before the
    // child, and keep its parent alive until after the child is destroyed.
    child: PresentationWindow,
    _window: std::rc::Rc<Window>,
}

impl Drop for D3dPresenter {
    fn drop(&mut self) {
        self.waiter.take();
        unsafe {
            self.context.ClearState();
            self.context.Flush();
            let _ = CloseHandle(self.ready);
        }
    }
}

unsafe fn compile(entry: PCSTR, target: PCSTR) -> Result<ID3DBlob> {
    let source = include_bytes!("d3d_present.hlsl");
    let mut code = None;
    let mut messages = None;
    if let Err(e) = D3DCompile(
        source.as_ptr().cast(),
        source.len(),
        s!("d3d_present.hlsl"),
        None,
        None,
        entry,
        target,
        D3DCOMPILE_OPTIMIZATION_LEVEL3,
        0,
        &mut code,
        Some(&mut messages),
    ) {
        let detail = messages
            .map(|b| {
                String::from_utf8_lossy(std::slice::from_raw_parts(
                    b.GetBufferPointer().cast(),
                    b.GetBufferSize(),
                ))
                .into_owned()
            })
            .unwrap_or_default();
        return Err(format!("{e}: {detail}"));
    }
    code.ok_or("missing shader bytecode".into())
}

impl D3dPresenter {
    pub fn new(
        window: std::rc::Rc<Window>,
        wake: winit::event_loop::EventLoopProxy<()>,
    ) -> Result<Self> {
        let RawWindowHandle::Win32(handle) =
            window.window_handle().map_err(|e| e.to_string())?.as_raw()
        else {
            return Err("not a Win32 window".into());
        };
        // The winit window outlives this presenter. All COM resources are owned
        // by this object, used on its creating thread, and released via RAII.
        unsafe {
            let mut device = None;
            let mut context = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE(0),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .map_err(error)?;
            let device = device.ok_or("missing D3D device")?;
            let context = context.ok_or("missing D3D context")?;
            let dxgi: IDXGIDevice = device.cast().map_err(error)?;
            let adapter = dxgi.GetAdapter().map_err(error)?;
            let factory: IDXGIFactory2 = adapter.GetParent().map_err(error)?;
            let hwnd = HWND(handle.hwnd.get());
            let child = PresentationWindow(CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                None,
                WS_CHILD | WS_VISIBLE | WS_DISABLED,
                0,
                0,
                1,
                1,
                hwnd,
                None,
                None,
                None,
            ));
            if child.0 == HWND(0) {
                return Err(error(windows::core::Error::from_win32()));
            }
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: 1,
                Height: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                AlphaMode: DXGI_ALPHA_MODE_IGNORE,
                Flags: DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT.0 as u32,
                ..Default::default()
            };
            let swap: IDXGISwapChain2 = factory
                .CreateSwapChainForHwnd(&device, child.0, &desc, None, None)
                .map_err(error)?
                .cast()
                .map_err(error)?;
            factory
                .MakeWindowAssociation(hwnd, DXGI_MWA_NO_ALT_ENTER)
                .map_err(error)?;
            swap.SetMaximumFrameLatency(1).map_err(error)?;
            let mut vertex = None;
            let mut pixel = None;
            let blob = compile(s!("vertex"), s!("vs_5_0"))?;
            device
                .CreateVertexShader(
                    std::slice::from_raw_parts(
                        blob.GetBufferPointer().cast(),
                        blob.GetBufferSize(),
                    ),
                    None,
                    Some(&mut vertex),
                )
                .map_err(error)?;
            let blob = compile(s!("pixel"), s!("ps_5_0"))?;
            device
                .CreatePixelShader(
                    std::slice::from_raw_parts(
                        blob.GetBufferPointer().cast(),
                        blob.GetBufferSize(),
                    ),
                    None,
                    Some(&mut pixel),
                )
                .map_err(error)?;
            let mut raster = None;
            device
                .CreateRasterizerState(
                    &D3D11_RASTERIZER_DESC {
                        FillMode: D3D11_FILL_SOLID,
                        CullMode: D3D11_CULL_NONE,
                        DepthClipEnable: true.into(),
                        ..Default::default()
                    },
                    Some(&mut raster),
                )
                .map_err(error)?;
            let mut constants = None;
            device
                .CreateBuffer(
                    &D3D11_BUFFER_DESC {
                        ByteWidth: 32,
                        Usage: D3D11_USAGE_DYNAMIC,
                        BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                        CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                        ..Default::default()
                    },
                    None,
                    Some(&mut constants),
                )
                .map_err(error)?;
            let ready = swap.GetFrameLatencyWaitableObject();
            if ready.is_invalid() {
                return Err("no DXGI frame latency event".into());
            }
            let waiter = match FrameWaiter::new(ready, move || wake.send_event(()).is_ok()) {
                Ok(waiter) => waiter,
                Err(message) => {
                    let _ = CloseHandle(ready);
                    return Err(message);
                }
            };
            Ok(Self {
                device,
                context,
                swap,
                ready,
                waiter: Some(waiter),
                queue_slot: false,
                retry_at: None,
                target: None,
                size: (1, 1),
                vertex: vertex.unwrap(),
                pixel: pixel.unwrap(),
                raster: raster.unwrap(),
                constants: constants.unwrap(),
                cells: None,
                detail: None,
                verified_size: None,
                frames: 0,
                child,
                _window: window,
            })
        }
    }

    pub fn cancel_retry(&mut self) {
        self.retry_at = None;
    }

    pub fn retry_at(&self) -> Option<std::time::Instant> {
        self.retry_at
    }

    /// False means the previous frame is still queued; retry with the newest
    /// guest state on the readiness notification. No old-image queue accumulates.
    pub fn present(
        &mut self,
        frame: &CompactPresentation,
        size: (u32, u32),
        rect: (u32, u32, u32, u32),
    ) -> Result<bool> {
        let (ox, oy, dw, dh) = rect;
        let sw = u64::from(frame.width) * u64::from(frame.scale);
        let sh = u64::from(frame.height) * u64::from(frame.scale);
        let total =
            (if sw > u64::from(dw) { sw } else { 1 }) * (if sh > u64::from(dh) { sh } else { 1 });
        if [
            sw,
            sh,
            u64::from(size.0),
            u64::from(size.1),
            u64::from(dw),
            u64::from(dh),
        ]
        .iter()
        .any(|&n| n == 0 || n > 16384)
            || total * 255 + total / 2 > u64::from(u32::MAX)
        {
            return Err(
                "coverage dimensions exceed exact 32-bit GPU arithmetic; using software".into(),
            );
        }
        unsafe {
            if !self.queue_slot {
                if !self.waiter.as_ref().unwrap().is_ready()? {
                    let _waiting = super::FramePhaseTimer::new("GPU waiting for queue slot");
                    return Ok(false);
                }
                self.queue_slot = true;
            }
            if self.size != size || self.target.is_none() {
                self.context.OMSetRenderTargets(None, None);
                self.target = None;
                self.swap
                    .ResizeBuffers(
                        2,
                        size.0,
                        size.1,
                        DXGI_FORMAT_B8G8R8A8_UNORM,
                        DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT.0 as u32,
                    )
                    .map_err(error)?;
                let back: ID3D11Texture2D = self.swap.GetBuffer(0).map_err(error)?;
                self.device
                    .CreateRenderTargetView(&back, None, Some(&mut self.target))
                    .map_err(error)?;
                SetWindowPos(
                    self.child.0,
                    None,
                    0,
                    0,
                    size.0 as i32,
                    size.1 as i32,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                )
                .map_err(error)?;
                self.size = size;
            }
            let _timing = super::FramePhaseTimer::new("GPU upload and submission");
            for (slot, data) in [
                (&mut self.cells, frame.cells.as_slice()),
                (&mut self.detail, frame.detail.as_slice()),
            ] {
                if slot.as_ref().is_none_or(|b| b.capacity < data.len()) {
                    *slot = Some(Upload::new(&self.device, data.len())?);
                }
                slot.as_ref().unwrap().write(&self.context, data)?;
            }
            let dimensions = [frame.width, frame.height, frame.scale, dw, dh, ox, oy, 0];
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(
                    &self.constants,
                    0,
                    D3D11_MAP_WRITE_DISCARD,
                    0,
                    Some(&mut mapped),
                )
                .map_err(error)?;
            std::ptr::copy_nonoverlapping(dimensions.as_ptr(), mapped.pData.cast(), 8);
            self.context.Unmap(&self.constants, 0);
            self.context
                .ClearRenderTargetView(self.target.as_ref().unwrap(), &[0., 0., 0., 1.]);
            self.context
                .OMSetRenderTargets(Some(&[self.target.clone()]), None);
            self.context.RSSetState(&self.raster);
            self.context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: ox as f32,
                TopLeftY: oy as f32,
                Width: dw as f32,
                Height: dh as f32,
                MinDepth: 0.,
                MaxDepth: 1.,
            }]));
            self.context
                .IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            self.context.VSSetShader(&self.vertex, None);
            self.context.PSSetShader(&self.pixel, None);
            self.context
                .PSSetConstantBuffers(0, Some(&[Some(self.constants.clone())]));
            self.context.PSSetShaderResources(
                0,
                Some(&[
                    Some(self.cells.as_ref().unwrap().view.clone()),
                    Some(self.detail.as_ref().unwrap().view.clone()),
                ]),
            );
            self.context.Draw(3, 0);
            self.context.PSSetShaderResources(0, Some(&[None, None]));
            if std::env::var_os("SYSTEMLESS_GPU_VERIFY").is_some()
                && (self.verified_size != Some(size) || self.frames % 240 == 0)
            {
                self.verify(frame, rect)?;
                self.verified_size = Some(size);
            }
            // Keep the latest completed image eligible for composition. A
            // sync interval of one plus a zero-time readiness poll missed
            // display slots when the 60.15 Hz guest and 60 Hz host drifted.
            // No ALLOW_TEARING flag: this is still a composed window.
            let result = self.swap.Present(0, DXGI_PRESENT_DO_NOT_WAIT);
            if result == DXGI_ERROR_WAS_STILL_DRAWING {
                // A ready queue slot does not guarantee Present itself cannot
                // return busy. No new latency signal is owed in this case.
                self.retry_at =
                    Some(std::time::Instant::now() + std::time::Duration::from_millis(1));
                let _busy = super::FramePhaseTimer::new("GPU Present busy");
                return Ok(false);
            }
            result.ok().map_err(error)?;
            // Retain this permission across WAS_STILL_DRAWING retries: the
            // latency event was already consumed, but no present was queued.
            self.retry_at = None;
            self.queue_slot = false;
            self.waiter.as_ref().unwrap().arm()?;
            let _accepted = super::FramePhaseTimer::new("GPU present accepted");
            self.frames += 1;
            Ok(true)
        }
    }

    /// Diagnostic only: synchronous readback and a full CPU oracle. Never
    /// enable this environment switch during performance measurements.
    unsafe fn verify(&self, frame: &CompactPresentation, rect: (u32, u32, u32, u32)) -> Result<()> {
        let back: ID3D11Texture2D = self.swap.GetBuffer(0).map_err(error)?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        back.GetDesc(&mut desc);
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        desc.MiscFlags = 0;
        let mut staging = None;
        self.device
            .CreateTexture2D(&desc, None, Some(&mut staging))
            .map_err(error)?;
        let staging = staging.unwrap();
        self.context.CopyResource(&staging, &back);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        self.context
            .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .map_err(error)?;
        let mut actual = Vec::with_capacity((desc.Width * desc.Height) as usize);
        for y in 0..desc.Height as usize {
            actual.extend_from_slice(std::slice::from_raw_parts(
                (mapped.pData as *const u8)
                    .add(y * mapped.RowPitch as usize)
                    .cast::<u32>(),
                desc.Width as usize,
            ));
        }
        self.context.Unmap(&staging, 0);
        let sw = frame.width * frame.scale;
        let sh = frame.height * frame.scale;
        let mut full = Vec::with_capacity((sw * sh) as usize);
        for y in 0..sh {
            for x in 0..sw {
                let cell =
                    frame.cells[((y / frame.scale) * frame.width + x / frame.scale) as usize];
                let rgb = if cell >> 31 == 0 {
                    cell
                } else {
                    frame.detail[(cell & 0x7fffffff) as usize
                        + ((y % frame.scale) * frame.scale + x % frame.scale) as usize]
                };
                full.push(rgb | 0xff000000);
            }
        }
        let (ox, oy, dw, dh) = rect;
        let mut expected = Vec::new();
        systemless::display::resize_argb_coverage(&full, (sw, sh), (dw, dh), &mut expected);
        for y in 0..desc.Height {
            for x in 0..desc.Width {
                let wanted = if x >= ox && x < ox + dw && y >= oy && y < oy + dh {
                    expected[((y - oy) * dw + x - ox) as usize]
                } else {
                    0xff000000
                };
                let got = actual[(y * desc.Width + x) as usize];
                if got != wanted {
                    return Err(format!(
                        "GPU mismatch ({x},{y}) got {got:08x}, expected {wanted:08x}"
                    ));
                }
            }
        }
        if let Some(directory) = std::env::var_os("SYSTEMLESS_GPU_CAPTURE_DIR") {
            let rgba: Vec<u8> = actual
                .iter()
                .flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8, 255])
                .collect();
            let path = std::path::PathBuf::from(directory).join(format!(
                "gpu-{}-{}x{}.png",
                self.frames, desc.Width, desc.Height
            ));
            image::save_buffer(
                path,
                &rgba,
                desc.Width,
                desc.Height,
                image::ColorType::Rgba8,
            )
            .map_err(|e| e.to_string())?;
        }
        eprintln!(
            "[GPU-VERIFY] frame={} size={}x{} exact; logical={} detail={} bytes",
            self.frames,
            desc.Width,
            desc.Height,
            frame.cells.len() * 4,
            frame.detail.len() * 4
        );
        Ok(())
    }
}
