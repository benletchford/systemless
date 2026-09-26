use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

#[wasm_bindgen(inline_js = r#"
export function createSystemlessRenderer(canvas, generation) {
  if (new URLSearchParams(location.search).get('renderer') !== 'worker'
      || typeof canvas.transferControlToOffscreen !== 'function'
      || typeof ResizeObserver !== 'function' || typeof Worker !== 'function') return null;
  const resources = performance.getEntriesByType('resource').map(entry => entry.name);
  const version = resources.find(url => /systemless[^/]*_bg\.wasm(?:\?|$)/.test(url)) || 'development';
  const moduleUrl = new URL('/renderer-client.js', location.href);
  const workerUrl = new URL('/renderer-worker.js', location.href);
  moduleUrl.searchParams.set('runtime', version);
  workerUrl.search = moduleUrl.search;
  const handle = { client: null, pending: null, error: null, disposed: false, timer: null };
  let last = performance.now(), waiting = 0;
  handle.timer = setInterval(() => {
    const now = performance.now();
    if (document.visibilityState !== 'hidden') waiting += Math.max(0, now - last);
    last = now;
    if (waiting >= 10000) {
      handle.error = 'Renderer module startup timed out';
      clearInterval(handle.timer);
      handle.timer = null;
    }
  }, 500);
  // Observe both import/initialization rejection and cancellation after import.
  import(moduleUrl.href).then(({ RendererClient }) => {
    if (handle.disposed || handle.error) return;
    handle.client = new RendererClient(canvas, workerUrl.href, generation, {
      backend: new URLSearchParams(location.search).get("renderer_gpu") === "1" ? "webgl" : "canvas2d"
    });
    const frame = handle.pending;
    handle.pending = null;
    if (frame) handle.client.paint(frame.width, frame.height, frame.pixels);
  }).catch(error => { if (!handle.disposed) handle.error = String(error?.message || error); })
    .finally(() => { clearInterval(handle.timer); handle.timer = null; });
  return handle;
}

export function paintSystemlessRenderer(handle, width, height, pixels) {
  if (handle.disposed || handle.error) return;
  if (handle.client) handle.client.paint(width, height, pixels);
  else handle.pending = { width, height, pixels };
}

export function systemlessRendererStatus(handle) {
  if (handle.error) return { phase: 'failed', error: handle.error, submitted: false, pending: false };
  return handle.client?.status() || { phase: 'booting', submitted: false, pending: !!handle.pending };
}

export function takeSystemlessRendererRecovery(handle) {
  const frame = handle.client?.takeRecoveryFrame() || handle.pending;
  handle.pending = null;
  return frame || null;
}

export function disposeSystemlessRenderer(handle) {
  if (handle.disposed) return;
  handle.disposed = true;
  clearInterval(handle.timer);
  handle.timer = null;
  handle.client?.dispose();
  handle.client = handle.pending = null;
}
"#)]
extern "C" {
    #[wasm_bindgen(catch, js_name = createSystemlessRenderer)]
    pub fn create_renderer(canvas: &HtmlCanvasElement, generation: u32)
        -> Result<JsValue, JsValue>;
    #[wasm_bindgen(js_name = paintSystemlessRenderer)]
    pub fn paint_renderer(handle: &JsValue, width: u32, height: u32, pixels: &Uint8Array);
    #[wasm_bindgen(js_name = systemlessRendererStatus)]
    pub fn renderer_status(handle: &JsValue) -> JsValue;
    #[wasm_bindgen(js_name = takeSystemlessRendererRecovery)]
    pub fn take_recovery(handle: &JsValue) -> JsValue;
    #[wasm_bindgen(js_name = disposeSystemlessRenderer)]
    pub fn dispose_renderer(handle: &JsValue);
}
