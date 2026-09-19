use js_sys::{Array, Object};
use wasm_bindgen::{prelude::wasm_bindgen, JsValue};
use web_sys::Worker;

#[wasm_bindgen(inline_js = r#"
const archivePrefetches = new Map();

function validateArchiveResponse(response) {
  if (!response.ok) {
    throw new Error("HTTP " + response.status);
  }
  const contentType = (response.headers.get("content-type") || "").toLowerCase();
  if (contentType.includes("text/html")) {
    throw new Error("archive URL returned HTML instead of game data");
  }
  return response;
}

function archiveFetchPromise(url) {
  let promise = archivePrefetches.get(url);
  if (promise) {
    return promise;
  }
  promise = fetch(url)
    .then(validateArchiveResponse)
    .then((response) => response.arrayBuffer())
    .catch((error) => {
      archivePrefetches.delete(url);
      throw error;
    });
  archivePrefetches.set(url, promise);
  return promise;
}

export function prefetchArchive(url) {
  archiveFetchPromise(url).catch(() => {});
}

export function prefetchedArchive(url) {
  return archivePrefetches.get(url) || null;
}

export async function fetchArchiveInWorker(url) {
  const source = `
self.onmessage = async (event) => {
  try {
    const response = await fetch(event.data);
    if (!response.ok) {
      throw new Error("HTTP " + response.status);
    }
    const contentType = (response.headers.get("content-type") || "").toLowerCase();
    if (contentType.includes("text/html")) {
      throw new Error("archive URL returned HTML instead of game data");
    }
    const buffer = await response.arrayBuffer();
    self.postMessage({ ok: true, buffer }, [buffer]);
  } catch (error) {
    const message = error && error.message ? error.message : String(error);
    self.postMessage({ ok: false, error: message });
  }
};`;
  const workerUrl = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
  return await new Promise((resolve, reject) => {
    const worker = new Worker(workerUrl);
    const cleanup = () => {
      worker.terminate();
      URL.revokeObjectURL(workerUrl);
    };
    worker.onmessage = (event) => {
      cleanup();
      if (event.data && event.data.ok) {
        resolve(event.data.buffer);
      } else {
        reject(new Error((event.data && event.data.error) || "worker fetch failed"));
      }
    };
    worker.onerror = (event) => {
      cleanup();
      reject(new Error(event.message || "worker fetch error"));
    };
    worker.postMessage(url);
  });
}

export function systemlessRuntimeAssets() {
  const resources = performance.getEntriesByType("resource").map((entry) => entry.name);
  const wasmUrl = resources.find((url) => /systemless[^/]*_bg\.wasm(?:\?|$)/.test(url));
  let moduleUrl = resources.find((url) => /systemless[^/]*\.js(?:\?|$)/.test(url));
  if (!moduleUrl) {
    moduleUrl = [...document.scripts]
      .map((script) => script.src)
      .find((url) => /systemless[^/]*\.js(?:\?|$)/.test(url));
  }
  return moduleUrl && wasmUrl ? [moduleUrl, wasmUrl] : [];
}

export function bootSystemlessWorker(worker, message, transfer, timeoutMs) {
  return new Promise((resolve, reject) => {
    const finish = (callback, value) => {
      clearTimeout(timer);
      worker.onmessage = null;
      worker.onerror = null;
      worker.onmessageerror = null;
      callback(value);
    };
    const timer = setTimeout(
      () => finish(reject, new Error("PowerPC worker startup timed out")),
      timeoutMs,
    );
    worker.onmessage = (event) => {
      const data = event.data || {};
      if (data.type === "ready") finish(resolve, data);
      else if (data.type === "error") {
        finish(reject, new Error(data.message || "PowerPC worker startup failed"));
      }
    };
    worker.onerror = (event) => {
      finish(reject, new Error(event.message || "PowerPC worker failed to load"));
    };
    worker.onmessageerror = () => {
      finish(reject, new Error("PowerPC worker returned an unreadable message"));
    };
    try {
      worker.postMessage(message, transfer);
    } catch (error) {
      finish(reject, error);
    }
  });
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = prefetchArchive)]
    pub fn prefetch_archive(url: &str);
    #[wasm_bindgen(js_name = prefetchedArchive)]
    pub fn prefetched_archive(url: &str) -> JsValue;
    #[wasm_bindgen(catch, js_name = fetchArchiveInWorker)]
    pub fn fetch_archive_in_worker(url: &str) -> Result<js_sys::Promise, JsValue>;
    #[wasm_bindgen(js_name = systemlessRuntimeAssets)]
    pub fn systemless_runtime_assets() -> Array;
    #[wasm_bindgen(js_name = bootSystemlessWorker)]
    pub fn boot_systemless_worker(
        worker: &Worker,
        message: &Object,
        transfer: &Array,
        timeout_ms: u32,
    ) -> js_sys::Promise;
}
