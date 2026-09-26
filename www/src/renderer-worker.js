// Complete-frame presenter protocol. Guest execution, composition and display
// demand stay on the execution owner/host. This worker has no Wasm or DOM state.
const RENDER_PROTOCOL = 1;
const MAX_PIXELS = 16 * 1024 * 1024;
let identity = null;
let canvas = null;
let context = null;
let pending = null;
let scheduled = null;
let failed = false;
let sequence = 0;
let displayGeneration = -1;
let dimensions = null;

function reply(message, transfer = []) {
  self.postMessage({ ...message, ...identity, protocolVersion: RENDER_PROTOCOL }, transfer);
}

function fail(error) {
  if (failed) return;
  failed = true;
  if (scheduled !== null) clearTimeout(scheduled);
  scheduled = null;
  pending = null;
  context = null;
  reply({ type: "error", message: String(error?.message || error) });
}

function sameIdentity(message) {
  return identity && message.generation === identity.generation
    && message.rendererGeneration === identity.rendererGeneration;
}

function acceptFrame(message) {
  if (!Number.isSafeInteger(message.sequence) || message.sequence <= sequence) return;
  const { width, height, displayGeneration: mode, pixels } = message;
  if (message.kind !== "rgba" || message.complete !== true || !Number.isSafeInteger(width) || !Number.isSafeInteger(height)
      || width < 1 || height < 1 || width > 8192 || height > 8192
      || width * height > MAX_PIXELS || !Number.isSafeInteger(mode) || mode < displayGeneration
      || !(pixels instanceof Uint8Array) || pixels.byteLength !== width * height * 4
      || !(pixels.buffer instanceof ArrayBuffer)) {
    throw new Error("Invalid complete RGBA presentation packet");
  }
  if (dimensions && mode === displayGeneration
      && (width !== dimensions[0] || height !== dimensions[1])) {
    throw new Error("Display dimensions changed without a new display generation");
  }
  sequence = message.sequence;
  displayGeneration = mode;
  dimensions = [width, height];
  if (pending) {
    reply({ type: "dropped", sequence: pending.sequence, buffer: pending.pixels.buffer },
      [pending.pixels.buffer]);
  }
  pending = message;
  if (scheduled === null) scheduled = setTimeout(paint, 0);
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
    const rgba = new Uint8ClampedArray(frame.pixels.buffer, frame.pixels.byteOffset, frame.pixels.byteLength);
    context.putImageData(new ImageData(rgba, frame.width, frame.height), 0, 0);
    // This acknowledges submission, not physical display or GPU completion.
    // Duration uses one worker clock; the sender measures transport round trips.
    reply({ type: "submitted", sequence: frame.sequence, displayGeneration: frame.displayGeneration,
      renderMs: performance.now() - start, buffer: frame.pixels.buffer }, [frame.pixels.buffer]);
  } catch (error) {
    fail(error);
  }
}

self.onmessage = ({ data }) => {
  if (failed) return;
  try {
    if (data?.type === "init") {
      if (identity) throw new Error("Renderer already initialized");
      if (data.protocolVersion !== RENDER_PROTOCOL || !Number.isSafeInteger(data.generation)
          || data.generation < 0 || !Number.isSafeInteger(data.rendererGeneration)
          || data.rendererGeneration < 0) throw new Error("Renderer protocol mismatch");
      identity = { generation: data.generation, rendererGeneration: data.rendererGeneration };
      canvas = data.canvas;
      context = canvas?.getContext("2d", { alpha: false });
      if (!context) throw new Error("OffscreenCanvas 2D is unavailable");
      canvas.addEventListener("contextlost", event => { event.preventDefault(); fail("Renderer context lost"); });
      reply({ type: "ready", backend: "offscreen-canvas2d", kinds: ["rgba"] });
      return;
    }
    if (!sameIdentity(data) || data.protocolVersion !== RENDER_PROTOCOL) return;
    if (data.type === "frame") acceptFrame(data);
    else if (data.type === "stop") {
      if (scheduled !== null) clearTimeout(scheduled);
      pending = null;
      scheduled = null;
      context = null;
      canvas.width = canvas.height = 1;
      reply({ type: "stopped" });
      failed = true;
      self.close();
    }
  } catch (error) {
    fail(error);
  }
};
self.onmessageerror = () => fail("Unreadable renderer message");
