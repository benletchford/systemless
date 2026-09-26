use js_sys::{Array, Object};
use wasm_bindgen::{prelude::wasm_bindgen, JsValue};
use web_sys::Worker;

#[wasm_bindgen(inline_js = r#"
const archivePrefetches = new Map();
const archiveFetchCancellations = new WeakMap();
const MAX_ARCHIVE_PREFETCHES = 2;

export function cancelArchiveFetch(promise) {
  archiveFetchCancellations.get(promise)?.();
}

function ownArchiveFetch(promise, cancel) {
  archiveFetchCancellations.set(promise, cancel);
  promise.then(() => archiveFetchCancellations.delete(promise),
    () => archiveFetchCancellations.delete(promise));
  return promise;
}

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

export function prefetchArchive(url) {
  if (archivePrefetches.has(url)) return;
  while (archivePrefetches.size >= MAX_ARCHIVE_PREFETCHES) {
    const [oldUrl, old] = archivePrefetches.entries().next().value;
    archivePrefetches.delete(oldUrl);
    old.controller.abort();
  }
  const controller = new AbortController();
  const entry = { controller, promise: null };
  entry.promise = ownArchiveFetch(fetch(url, { signal: controller.signal })
    .then(validateArchiveResponse)
    .then(response => response.arrayBuffer()), () => controller.abort());
  archivePrefetches.set(url, entry);
  entry.promise.catch(() => {
    if (archivePrefetches.get(url) === entry) archivePrefetches.delete(url);
  });
}

export function prefetchedArchive(url) {
  const entry = archivePrefetches.get(url);
  archivePrefetches.delete(url);
  return entry?.promise || null;
}

export function fetchArchiveInWorker(url) {
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
  let worker;
  try {
    worker = new Worker(workerUrl);
  } catch (error) {
    URL.revokeObjectURL(workerUrl);
    throw error;
  }
  let cancel;
  const promise = new Promise((resolve, reject) => {
    let finished = false;
    const finish = (settle, value) => {
      if (finished) return;
      finished = true;
      worker.onmessage = worker.onerror = worker.onmessageerror = null;
      worker.terminate();
      URL.revokeObjectURL(workerUrl);
      settle(value);
    };
    cancel = () => finish(reject, new Error("Archive fetch cancelled"));
    worker.onmessage = (event) => {
      if (event.data && event.data.ok) {
        finish(resolve, event.data.buffer);
      } else {
        finish(reject, new Error((event.data && event.data.error) || "worker fetch failed"));
      }
    };
    worker.onerror = event => finish(reject, new Error(event.message || "worker fetch error"));
    worker.onmessageerror = () => finish(reject, new Error("Unreadable archive fetch reply"));
    try { worker.postMessage(url); } catch (error) { finish(reject, error); }
  });
  return ownArchiveFetch(promise, cancel);
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

export function observeSystemlessPromise(promise) {
  // Preloaded audio work can reject after a cancelled boot drops its awaiter.
  // Observe the rejection without changing the promise used by normal startup.
  promise.catch(() => {});
}

export function yieldSystemlessTask() {
  return new Promise(resolve => setTimeout(resolve, 0));
}

// Bound the browser's opaque Worker message queue as well as our own queue.
// Movement can replace only another trailing movement, never a key/button/save
// boundary. A full transition queue fails visibly instead of dropping an up.
const workerCommandQueues = new WeakMap();
const MAX_IN_FLIGHT_COMMANDS = 8;
const MAX_PENDING_COMMANDS = 256;

function drainWorkerCommands(worker, queue) {
  while (queue.pending.length && queue.inFlight.size < MAX_IN_FLIGHT_COMMANDS) {
    const { message, transfer } = queue.pending.shift();
    if (queue.sequence === Number.MAX_SAFE_INTEGER) throw new Error("Worker command sequence exhausted");
    const commandSequence = ++queue.sequence;
    queue.inFlight.add(commandSequence);
    try {
      worker.postMessage({ ...message, commandSequence }, transfer);
    } catch (error) {
      queue.pending.length = 0;
      workerCommandQueues.delete(worker);
      throw error;
    }
  }
}

export function postSystemlessWorkerCommand(worker, message, transfer) {
  let queue = workerCommandQueues.get(worker);
  if (!queue) {
    queue = { generation: message.generation, sequence: 0, inFlight: new Set(), pending: [], shuttingDown: false };
    workerCommandQueues.set(worker, queue);
  }
  if (queue.generation !== message.generation) throw new Error("Stale worker command generation");
  if (queue.shuttingDown) throw new Error("Runtime worker is shutting down");
  if (message.type === "shutdown") queue.shuttingDown = true;
  const last = queue.pending.at(-1);
  if (message.type === "mouseMove" && last?.message.type === "mouseMove") {
    last.message = message;
  } else {
    if (queue.pending.length >= MAX_PENDING_COMMANDS && message.type !== "shutdown") throw new Error("Runtime input queue is full");
    queue.pending.push({ message, transfer });
  }
  drainWorkerCommands(worker, queue);
}

export function acknowledgeSystemlessWorkerCommand(worker, generation, sequence) {
  const queue = workerCommandQueues.get(worker);
  if (!queue || generation !== queue.generation || !queue.inFlight.delete(sequence)) return;
  drainWorkerCommands(worker, queue);
}

const workerBootCancellation = new WeakMap();
const workerShutdownCancellation = new WeakMap();
const pendingGameShutdowns = new Map();

export function waitSystemlessWorkerShutdown(gameId) {
  return pendingGameShutdowns.get(gameId) || Promise.resolve();
}

export function shutdownSystemlessWorker(worker, generation, gameId) {
  const promise = new Promise((resolve, reject) => {
    let settled = false;
    let recoverableError = null;
    const finish = (error) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      workerShutdownCancellation.delete(worker);
      workerCommandQueues.delete(worker);
      worker.onmessage = worker.onerror = worker.onmessageerror = null;
      worker.terminate();
      if (error) reject(error); else resolve();
    };
    const timer = setTimeout(() => finish(new Error("Timed out saving the previous game")), 15000);
    workerShutdownCancellation.set(worker, () => finish(new Error("Game stopped before saving completed")));
    worker.onmessage = ({ data }) => {
      if (data?.generation !== generation) return;
      if (data.type === "stopped") finish(recoverableError);
      else if (data.type === "error") {
        const error = new Error(data.message || "Could not save the previous game");
        if (data.fatal === false) recoverableError = error;
        else finish(error);
      }
      else if (data.type === "commandAck") {
        try { acknowledgeSystemlessWorkerCommand(worker, generation, data.commandSequence); }
        catch (error) { finish(error); }
      }
    };
    worker.onerror = event => finish(new Error(event.message || "Worker stopped before saving completed"));
    worker.onmessageerror = () => finish(new Error("Unreadable save completion"));
    try { postSystemlessWorkerCommand(worker, { type: "shutdown", generation }, []); }
    catch (error) { finish(error); }
  });
  const finished = promise.then(
    () => { if (pendingGameShutdowns.get(gameId) === finished) pendingGameShutdowns.delete(gameId); },
    error => {
      if (pendingGameShutdowns.get(gameId) === finished) pendingGameShutdowns.delete(gameId);
      throw error;
    },
  );
  pendingGameShutdowns.set(gameId, finished);
  return finished;
}

export function cancelSystemlessWorker(worker) {
  workerCommandQueues.delete(worker);
  workerBootCancellation.get(worker)?.();
  workerShutdownCancellation.get(worker)?.();
  worker.terminate();
}

export function bootSystemlessWorker(worker, message, transfer, timeoutMs, onProgress) {
  return new Promise((resolve, reject) => {
    let settled = false;
    const finish = (callback, value) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      workerBootCancellation.delete(worker);
      worker.onmessage = null;
      worker.onerror = null;
      worker.onmessageerror = null;
      callback(value);
    };
    const timer = setTimeout(
      () => finish(reject, new Error("Runtime worker startup timed out")),
      timeoutMs,
    );
    workerBootCancellation.set(worker, () => finish(reject, new Error("Runtime worker startup cancelled")));
    worker.onmessage = (event) => {
      const data = event.data || {};
      if (data.generation !== message.generation) return;
      if (data.protocolVersion !== message.protocolVersion) {
        finish(reject, new Error("Runtime worker assets use an incompatible protocol"));
      } else if (data.type === "ready") {
        finish(resolve, data);
      } else if (data.type === "progress") {
        onProgress(data.progress);
      } else if (data.type === "error") {
        finish(reject, new Error(data.message || "Runtime worker startup failed"));
      }
    };
    worker.onerror = (event) => {
      finish(reject, new Error(event.message || "Runtime worker failed to load"));
    };
    worker.onmessageerror = () => {
      finish(reject, new Error("Runtime worker returned an unreadable message"));
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
    #[wasm_bindgen(js_name = cancelArchiveFetch)]
    pub fn cancel_archive_fetch(promise: &js_sys::Promise);
    #[wasm_bindgen(js_name = prefetchArchive)]
    pub fn prefetch_archive(url: &str);
    #[wasm_bindgen(js_name = prefetchedArchive)]
    pub fn prefetched_archive(url: &str) -> JsValue;
    #[wasm_bindgen(catch, js_name = fetchArchiveInWorker)]
    pub fn fetch_archive_in_worker(url: &str) -> Result<js_sys::Promise, JsValue>;
    #[wasm_bindgen(js_name = systemlessRuntimeAssets)]
    pub fn systemless_runtime_assets() -> Array;
    #[wasm_bindgen(js_name = shutdownSystemlessWorker)]
    pub fn shutdown_systemless_worker(
        worker: &Worker,
        generation: u32,
        game_id: &str,
    ) -> js_sys::Promise;
    #[wasm_bindgen(js_name = waitSystemlessWorkerShutdown)]
    pub fn wait_systemless_worker_shutdown(game_id: &str) -> js_sys::Promise;
    #[wasm_bindgen(catch, js_name = postSystemlessWorkerCommand)]
    pub fn post_systemless_worker_command(
        worker: &Worker,
        message: &Object,
        transfer: &Array,
    ) -> Result<(), JsValue>;
    #[wasm_bindgen(catch, js_name = acknowledgeSystemlessWorkerCommand)]
    pub fn acknowledge_systemless_worker_command(
        worker: &Worker,
        generation: u32,
        sequence: f64,
    ) -> Result<(), JsValue>;
    #[wasm_bindgen(js_name = observeSystemlessPromise)]
    pub fn observe_systemless_promise(promise: &js_sys::Promise);
    #[wasm_bindgen(js_name = yieldSystemlessTask)]
    pub fn yield_systemless_task() -> js_sys::Promise;
    #[wasm_bindgen(js_name = cancelSystemlessWorker)]
    pub fn cancel_systemless_worker(worker: &Worker);
    #[wasm_bindgen(js_name = bootSystemlessWorker)]
    pub fn boot_systemless_worker(
        worker: &Worker,
        message: &Object,
        transfer: &Array,
        timeout_ms: u32,
        on_progress: &js_sys::Function,
    ) -> js_sys::Promise;
}
