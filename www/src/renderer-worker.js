// Complete-frame presenter protocol. Guest execution, composition and display
// demand stay on the execution owner/host. This worker has no Wasm or DOM state.
const RENDER_PROTOCOL = 4;
const MAX_PIXELS = 16 * 1024 * 1024;
let identity = null;
let ownerPort = null;
let canvas = null;
let context = null;
let gpu = null;
let ready = false;
let pending = null;
let scheduled = null;
let failed = false;
let sequence = 0;
let displayGeneration = -1;
let dimensions = null;

function reply(message, transfer = []) {
  const packet = { ...message, ...identity, protocolVersion: RENDER_PROTOCOL };
  if (ownerPort && ["submitted", "dropped"].includes(message.type)) ownerPort.postMessage(packet, transfer);
  else {
    self.postMessage(packet, transfer);
    if (ownerPort && message.type === "error") ownerPort.postMessage(packet);
  }
}

function fail(error) {
  if (failed) return;
  failed = true;
  if (scheduled !== null) clearTimeout(scheduled);
  scheduled = null;
  pending = null;
  context = null;
  gpu?.dispose(); gpu = null;
  reply({ type: "error", message: String(error?.message || error) });
}

function sameIdentity(message) {
  return identity && message.generation === identity.generation
    && message.rendererGeneration === identity.rendererGeneration;
}

function acceptFrame(message) {
  if (!Number.isSafeInteger(message.sequence) || message.sequence <= sequence) return;
  const { width, height, displayGeneration: mode, pixels } = message;
  const indexed = message.kind === "indexed8" && gpu;
  const compact = message.kind === "compact" && gpu;
  const validPixels = compact
    ? message.compact?.cells instanceof Uint32Array && message.compact.cells.buffer instanceof ArrayBuffer
      && message.compact.detail instanceof Uint32Array && message.compact.detail.buffer instanceof ArrayBuffer
    : indexed
    ? Number.isSafeInteger(message.stride) && message.stride >= width && message.stride <= 8192
      && message.stride * height <= MAX_PIXELS && pixels?.byteLength === message.stride * height
      && message.palette instanceof Uint8Array && message.palette.byteLength === 1024
      && message.palette.buffer instanceof ArrayBuffer
    : message.kind === "rgba" && pixels?.byteLength === width * height * 4;
  if (!ready || !validPixels || message.complete !== true || !Number.isSafeInteger(width) || !Number.isSafeInteger(height)
      || width < 1 || height < 1 || width > 8192 || height > 8192
      || width * height > MAX_PIXELS || !Number.isSafeInteger(mode) || mode < displayGeneration
      || (!compact && (!(pixels instanceof Uint8Array) || !(pixels.buffer instanceof ArrayBuffer)))) {
    throw new Error("Invalid complete presentation packet");
  }
  const layout = message.kind === "compact" ? `compact:${message.compact.width}:${message.compact.height}:${message.compact.scale}` : `${message.kind}:${message.kind === "indexed8" ? message.stride : width * 4}`;
  if (dimensions && mode === displayGeneration
      && (width !== dimensions[0] || height !== dimensions[1] || layout !== dimensions[2])) {
    throw new Error("Display dimensions changed without a new display generation");
  }
  sequence = message.sequence;
  displayGeneration = mode;
  dimensions = [width, height, layout];
  if (pending) {
    returnBuffers("dropped", pending);
  }
  pending = message;
  if (scheduled === null) scheduled = setTimeout(paint, 0);
}

function returnBuffers(type, frame, metrics = {}) {
  const buffer = frame.kind === "compact" ? frame.compact.cells.buffer : frame.pixels.buffer;
  const detailBuffer = frame.compact?.detail?.buffer;
  const paletteBuffer = frame.kind === "indexed8" ? frame.palette.buffer : undefined;
  const cursorBuffer = frame.cursor?.pixels?.buffer;
  const transfer = [...new Set([buffer, paletteBuffer, cursorBuffer, detailBuffer].filter(Boolean))];
  if (ownerPort && type === "submitted") {
    const bytes = [frame.pixels, frame.palette, frame.cursor?.pixels, frame.compact?.cells, frame.compact?.detail]
      .filter(Boolean).reduce((total, view) => total + view.byteLength, 0);
    // UI submission acknowledgement must not wait for the owner to finish its
    // next guest batch. Only the separate credit reply returns image buffers.
    reply({ type: "directSubmitted", sequence: frame.sequence, kind: frame.kind, bytes,
      width: frame.width, height: frame.height, outputScale: frame.outputScale, ...metrics });
  }
  reply({ type, sequence: frame.sequence, displayGeneration: frame.displayGeneration,
    ...metrics, buffer, paletteBuffer, cursorBuffer, detailBuffer }, transfer);
}

function paint() {
  scheduled = null;
  const frame = pending;
  pending = null;
  if (failed || !frame) return;
  try {
    const start = performance.now();
    if (canvas.width !== frame.width) canvas.width = frame.width;
    if (canvas.height !== frame.height) canvas.height = frame.height;
    if (gpu) gpu.paint(frame);
    else {
      const rgba = new Uint8ClampedArray(frame.pixels.buffer, frame.pixels.byteOffset, frame.pixels.byteLength);
      context.putImageData(new ImageData(rgba, frame.width, frame.height), 0, 0);
    }
    // This acknowledges submission, not physical display or GPU completion.
    // Duration uses one worker clock; the sender measures transport round trips.
    returnBuffers("submitted", frame, { renderMs: performance.now() - start });
  } catch (error) {
    fail(error);
  }
}

self.onmessage = async ({ data }) => {
  if (failed) return;
  try {
    if (data?.type === "init") {
      if (identity) throw new Error("Renderer already initialized");
      if (data.protocolVersion !== RENDER_PROTOCOL || !Number.isSafeInteger(data.generation)
          || data.generation < 0 || !Number.isSafeInteger(data.rendererGeneration)
          || data.rendererGeneration < 0) throw new Error("Renderer protocol mismatch");
      identity = { generation: data.generation, rendererGeneration: data.rendererGeneration };
      canvas = data.canvas;
      if (data.backend === "webgl") {
        const moduleUrl = new URL("./renderer-gpu.js", self.location.href);
        moduleUrl.search = new URL(self.location.href).search;
        const bindings = await import(moduleUrl.href);
        // Stop, duplicate init or message failure may have arrived during import.
        if (failed) return;
        if (bindings.GPU_PRESENTER_PROTOCOL !== 3) throw new Error("GPU presenter protocol mismatch");
        gpu = new bindings.GpuFramePresenter(canvas);
        canvas.addEventListener("webglcontextlost", event => { event.preventDefault(); fail("Renderer WebGL context lost"); });
      } else {
        context = canvas?.getContext("2d", { alpha: false });
        if (!context) throw new Error("OffscreenCanvas 2D is unavailable");
        canvas.addEventListener("contextlost", event => { event.preventDefault(); fail("Renderer context lost"); });
      }
      ready = true;
      reply({ type: "ready", backend: gpu ? "offscreen-webgl" : "offscreen-canvas2d",
        kinds: gpu ? ["rgba", "indexed8", "compact"] : ["rgba"] });
      return;
    }
    if (!sameIdentity(data) || data.protocolVersion !== RENDER_PROTOCOL) return;
    if (data.type === "connectOwner") {
      if (!ready || ownerPort || pending) throw new Error("Renderer owner handoff is not idle");
      ownerPort = data.port;
      ownerPort.onmessage = ({ data: frame }) => {
        if (failed || !sameIdentity(frame) || frame.protocolVersion !== RENDER_PROTOCOL) return;
        try { if (frame.type === "frame") acceptFrame(frame); }
        catch (error) { fail(error); }
      };
      ownerPort.onmessageerror = () => fail("Unreadable owner presentation packet");
      ownerPort.start();
    } else if (data.type === "frame") {
      if (ownerPort) throw new Error("Host images after direct owner handoff");
      acceptFrame(data);
    }
    else if (data.type === "stop") {
      if (scheduled !== null) clearTimeout(scheduled);
      pending = null;
      scheduled = null;
      context = null;
      gpu?.dispose(); gpu = null;
      canvas.width = canvas.height = 1;
      reply({ type: "stopped" });
      failed = true;
      ownerPort?.close();
      ownerPort = null;
      self.close();
    }
  } catch (error) {
    fail(error);
  }
};
self.onmessageerror = () => fail("Unreadable renderer message");
