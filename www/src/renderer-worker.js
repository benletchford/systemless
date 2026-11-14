// Complete-frame presenter protocol. Guest execution, composition and display
// demand stay on the execution owner/host. The wgpu backend owns a separate Wasm instance.
const RENDER_PROTOCOL = 5;
const MAX_PIXELS = 16 * 1024 * 1024;
const NATIVE_LITTLE_ENDIAN = new Uint8Array(new Uint32Array([0x01020304]).buffer)[0] === 4;
let identity = null;
let ownerPort = null;
let canvas = null;
let context = null;
let gpu = null;
let wgpu = null;
let painting = false;
let healthTimer = null;
let bitmapOutput = false;
let awaitingBitmap = null;
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
  awaitingBitmap = null;
  context = null;
  gpu?.dispose(); gpu = null;
  clearInterval(healthTimer); healthTimer = null;
  wgpu?.dispose(); wgpu?.free(); wgpu = null;
  reply({ type: "error", message: String(error?.message || error) });
}

function sameIdentity(message) {
  return identity && message.generation === identity.generation
    && message.rendererGeneration === identity.rendererGeneration;
}

function acceptFrame(message) {
  if (!Number.isSafeInteger(message.sequence) || message.sequence <= sequence) return;
  const { width, height, displayGeneration: mode, pixels } = message;
  const gpuCapable = Boolean(gpu || wgpu);
  const indexed = message.kind === "indexed8" && gpuCapable;
  const compact = message.kind === "compact" && gpuCapable;
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
  if (scheduled === null && !awaitingBitmap && !painting) scheduled = setTimeout(paint, 0);
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
      width: frame.width, height: frame.height, outputScale: frame.outputScale, cursorCss: frame.cursorCss, ...metrics });
  }
  reply({ type, sequence: frame.sequence, displayGeneration: frame.displayGeneration,
    ...metrics, buffer, paletteBuffer, cursorBuffer, detailBuffer }, transfer);
}

function compactBytes(words) {
  if (NATIVE_LITTLE_ENDIAN) return new Uint8Array(words.buffer, words.byteOffset, words.byteLength);
  const bytes = new Uint8Array(words.byteLength);
  const view = new DataView(bytes.buffer);
  for (let i = 0; i < words.length; i++) view.setUint32(i * 4, words[i], true);
  return bytes;
}

function paintWgpu(frame) {
  if (frame.kind === "compact") {
    const source = frame.compact;
    if (frame.cursor || !Number.isSafeInteger(source.width) || !Number.isSafeInteger(source.height)
        || !Number.isSafeInteger(source.scale) || source.width < 1 || source.height < 1
        || source.scale < 1 || source.scale > 4) {
      throw new Error("Invalid complete CompactPresentation");
    }
    wgpu.paint_compact(frame.width, frame.height, source.width, source.height, source.scale,
      compactBytes(source.cells), compactBytes(source.detail));
    return;
  }
  const cursor = frame.cursor;
  if (cursor && (!Number.isSafeInteger(cursor.x) || !Number.isSafeInteger(cursor.y)
      || !Number.isSafeInteger(cursor.width) || !Number.isSafeInteger(cursor.height)
      || cursor.x < 0 || cursor.y < 0 || cursor.width < 1 || cursor.height < 1
      || cursor.x + cursor.width > frame.width || cursor.y + cursor.height > frame.height
      || cursor.width * cursor.height > 4096 || !(cursor.pixels instanceof Uint8Array)
      || !(cursor.pixels.buffer instanceof ArrayBuffer)
      || cursor.pixels.byteLength !== cursor.width * cursor.height * 4)) {
    throw new Error("Invalid owned cursor patch");
  }
  if (frame.kind === "indexed8") {
    wgpu.paint_indexed8(frame.width, frame.height, frame.stride, frame.pixels, frame.palette,
      cursor ? cursor.x : 0, cursor ? cursor.y : 0, cursor ? cursor.width : 0, cursor ? cursor.height : 0,
      cursor ? cursor.pixels : new Uint8Array(0));
  } else if (cursor) {
    wgpu.paint_rgba_cursor(frame.width, frame.height, frame.pixels,
      cursor.x, cursor.y, cursor.width, cursor.height, cursor.pixels);
  } else {
    wgpu.paint(frame.width, frame.height, frame.pixels);
  }
}

async function paint() {
  scheduled = null;
  if (awaitingBitmap || painting) return;
  const frame = pending;
  pending = null;
  if (failed || !frame) return;
  painting = true;
  try {
    const start = performance.now();
    if (canvas.width !== frame.width) canvas.width = frame.width;
    if (canvas.height !== frame.height) canvas.height = frame.height;
    if (wgpu) {
      paintWgpu(frame);
      let timeout;
      try {
        await Promise.race([wgpu.completed(), new Promise((_, reject) => {
          timeout = setTimeout(() => reject(new Error("WebGPU frame completion timed out")), 10000);
        })]);
      } finally { clearTimeout(timeout); }
      if (failed) return;
    } else if (gpu) gpu.paint(frame);
    else {
      const rgba = new Uint8ClampedArray(frame.pixels.buffer, frame.pixels.byteOffset, frame.pixels.byteLength);
      context.putImageData(new ImageData(rgba, frame.width, frame.height), 0, 0);
    }
    if (bitmapOutput) {
      const bitmap = canvas.transferToImageBitmap();
      awaitingBitmap = { frame, renderMs: performance.now() - start };
      try {
        reply({ type: "bitmap", sequence: frame.sequence, displayGeneration: frame.displayGeneration,
          width: frame.width, height: frame.height, cursorCss: frame.cursorCss, bitmap }, [bitmap]);
      } catch (error) {
        bitmap.close();
        throw error;
      }
      // Keep frame credit until the host has submitted the bitmap. A stalled
      // host cannot accumulate a queue of transferred GPU images.
      return;
    }
    // wgpu also waits for GPU completion; neither path acknowledges physical display.
    // Duration uses one worker clock; the sender measures transport round trips.
    returnBuffers("submitted", frame, { renderMs: performance.now() - start });
  } catch (error) {
    fail(error);
  } finally {
    painting = false;
    if (!failed && pending && !awaitingBitmap && scheduled === null) scheduled = setTimeout(paint, 0);
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
      bitmapOutput = data.bitmap === true;
      if (bitmapOutput && (data.backend !== "webgl" || typeof canvas?.transferToImageBitmap !== "function")) {
        throw new Error("Bitmap presentation unavailable");
      }
      if (data.backend === "wgpu") {
        if (!data.moduleUrl || !data.wasmUrl) throw new Error("Missing wgpu Wasm assets");
        const bindings = await import(data.moduleUrl);
        if (failed) return;
        await bindings.default({ module_or_path: data.wasmUrl });
        if (failed) return;
        const presenter = await bindings.WebGpuPresenter.create(canvas);
        if (failed) { presenter.dispose(); presenter.free(); return; }
        wgpu = presenter;
        healthTimer = setInterval(() => {
          try { wgpu?.check_health(); } catch (error) { fail(error); }
        }, 1000);
      } else if (data.backend === "webgl") {
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
      reply({ type: "ready", backend: wgpu ? "offscreen-wgpu" : bitmapOutput ? "bitmap-webgl" : gpu ? "offscreen-webgl" : "offscreen-canvas2d", bitmap: bitmapOutput,
        kinds: (gpu || wgpu) ? ["rgba", "indexed8", "compact"] : ["rgba"] });
      return;
    }
    if (!sameIdentity(data) || data.protocolVersion !== RENDER_PROTOCOL) return;
    if (data.type === "bitmapSubmitted") {
      const active = awaitingBitmap;
      if (!active || data.sequence !== active.frame.sequence
          || data.displayGeneration !== active.frame.displayGeneration) return;
      awaitingBitmap = null;
      returnBuffers("submitted", active.frame, { renderMs: active.renderMs });
      if (pending && scheduled === null) scheduled = setTimeout(paint, 0);
    } else if (data.type === "connectOwner") {
      if (!ready || ownerPort || pending || awaitingBitmap || painting) throw new Error("Renderer owner handoff is not idle");
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
      awaitingBitmap = null;
      scheduled = null;
      context = null;
      gpu?.dispose(); gpu = null;
      clearInterval(healthTimer); healthTimer = null;
      wgpu?.dispose(); wgpu?.free(); wgpu = null;
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
