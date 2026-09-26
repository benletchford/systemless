const PROTOCOL_VERSION = 7;
let machine = null;
let generation = 0;
let frameSequence = 0;
let booting = false;
let failed = false;
let stopping = false;
let saveFilesVersion = null;
let rendererLink = null;

function disconnectRenderer() {
  const link = rendererLink;
  rendererLink = null;
  if (link) {
    link.cancelled = true;
    link.presenter?.dispose();
    link.port.close();
  }
}

async function connectRenderer(message) {
  disconnectRenderer();
  const link = rendererLink = { id: message.rendererGeneration, port: message.port, cancelled: false, presenter: null };
  try {
    if (message.rendererProtocol !== 4) throw new Error("Direct renderer protocol mismatch");
    const url = new URL("./renderer-owner.js", self.location.href);
    url.search = new URL(self.location.href).search;
    const bindings = await import(url.href);
    if (link.cancelled || stopping || failed) return;
    if (bindings.DIRECT_RENDERER_PROTOCOL !== 1) throw new Error("Direct transport assets mismatch");
    link.presenter = new bindings.RendererOwner(link.port,
      { generation, rendererGeneration: link.id },
      { sequence: message.sequence, displayGeneration: message.displayGeneration, notify: reply });
  } catch (error) {
    if (link.cancelled) return;
    reply({ type: "rendererStatus", rendererGeneration: link.id, event: "error", message: String(error?.message || error) });
    disconnectRenderer();
  }
}

function publishSaveFiles(type = "saveFiles", requestId) {
  const version = machine.saveFilesVersion();
  if (type === "saveFiles" && requestId === undefined && version === saveFilesVersion) return;
  const files = machine.saveFiles();
  saveFilesVersion = version;
  reply({ type, files, saveFilesVersion: version, requestId }, files.map(file => file.macbinary.buffer));
}

function reply(message, transfer = []) {
  self.postMessage({ ...message, generation, protocolVersion: PROTOCOL_VERSION }, transfer);
}

// Keep brief input down for six Macintosh ticks (100 ms of guest time).
// Host time can run ahead while a slow game draws before polling input.
const MIN_PRESS_TICKS = 6;
let guestTick = 0;
let uiTracking = false;
const pressed = new Map();
const pendingReleases = new Map();

function press(id, apply) {
  pendingReleases.get(id)?.();
  pendingReleases.delete(id);
  apply();
  if (!pressed.has(id)) pressed.set(id, { at: guestTick, ranGuest: false });
}

// Tracking loops consume input while guest time is frozen. Waiting for the
// minimum tick duration there would prevent MenuSelect from ever seeing up.
function canRelease(state) {
  return state.ranGuest && (uiTracking || ((guestTick - state.at) >>> 0) >= MIN_PRESS_TICKS);
}

function release(id, apply) {
  const state = pressed.get(id);
  const finish = () => { apply(); pressed.delete(id); };
  if (state && !canRelease(state)) {
    pendingReleases.set(id, finish);
  } else {
    pendingReleases.delete(id);
    finish();
  }
}

self.onmessage = async (event) => {
  const message = event.data || {};
  try {
    if (message.type === "boot") {
      if (booting || machine || stopping || failed) throw new Error("Runtime worker is already started");
      generation = message.generation;
      if (message.protocolVersion !== PROTOCOL_VERSION) {
        throw new Error("Runtime worker assets use an incompatible protocol");
      }
      booting = true;
      reply({ type: "progress", progress: JSON.stringify("LoadingExecutable") });
      const bindings = await import(message.moduleUrl);
      await bindings.default(message.wasmUrl);
      if (typeof bindings.WorkerMachine.runtimeProtocolVersion !== "function"
          || bindings.WorkerMachine.runtimeProtocolVersion() !== PROTOCOL_VERSION) {
        throw new Error("Runtime Wasm assets use an incompatible protocol");
      }
      machine = await bindings.WorkerMachine.create(
        new Uint8Array(message.gameBytes),
        message.config,
        message.pluginForks || [],
        (progress) => reply({ type: "progress", progress }),
      );
      booting = false;
      publishSaveFiles("ready");
      return;
    }
    if (message.generation !== generation || !machine || failed || stopping) return;
    if (message.type === "connectRenderer") { await connectRenderer(message); return; }
    if (message.type === "disconnectRenderer") {
      if (rendererLink?.id === message.rendererGeneration) disconnectRenderer();
      return;
    }
    if (message.type === "shutdown") {
      disconnectRenderer();
      stopping = true;
      await machine.flushSaves();
      machine.free();
      machine = null;
      reply({ type: "stopped" });
      return;
    }

    if (message.type === "frame") {
      const result = machine.runFrame(message.queuedAudioSamples ?? -1, !!message.debug, message.outputScale ?? 1, !!message.forceRender, !!message.indexedRender, !!message.compactRender, !!message.measurePresentation);
      guestTick = result.guestTick >>> 0;
      uiTracking = !!result.uiTracking;
      if (result.lastSteps > 0 || !result.running) {
        for (const state of pressed.values()) state.ranGuest = true;
      }
      for (const [id, apply] of pendingReleases) {
        const state = pressed.get(id);
        if (!result.running || canRelease(state)) {
          apply();
          pendingReleases.delete(id);
        }
      }
      if (message.directRender && rendererLink?.presenter) {
        try { rendererLink.presenter.submit(result); }
        catch (error) { rendererLink.presenter.fail(error); }
      }
      const transfer = [];
      if (result.frame) transfer.push(result.frame.buffer);
      if (result.audio) transfer.push(result.audio.buffer);
      if (result.compactFrame) transfer.push(result.compactFrame.compact.cells.buffer, result.compactFrame.compact.detail.buffer);
      if (result.indexedFrame) {
        transfer.push(result.indexedFrame.pixels.buffer, result.indexedFrame.palette.buffer);
        if (result.indexedFrame.cursor) transfer.push(result.indexedFrame.cursor.pixels.buffer);
      }
      if (result.gpuFrame) {
        for (const texture of result.gpuFrame.textures) transfer.push(texture.rgba.buffer);
        for (const draw of result.gpuFrame.draws) transfer.push(draw.vertices.buffer);
      }
      publishSaveFiles();
      reply({ type: "frame", ...result, sequence: ++frameSequence }, transfer);
      return;
    }
    if (message.type === "enableGpu") machine.setGpuRendererEnabled(!!message.enabled);
    if (message.type === "mouseDown") press("mouse", () => machine.mouseDown(message.v, message.h));
    else if (message.type === "mouseUp") release("mouse", () => machine.mouseUp(message.v, message.h));
    else if (message.type === "mouseMove") machine.mouseMove(message.v, message.h);
    else if (message.type === "keyDown") press(message.macKey, () => machine.keyDown(message.macKey, message.charCode));
    else if (message.type === "keyUp") release(message.macKey, () => machine.keyUp(message.macKey, message.charCode));
    else if (message.type === "importSave") {
      machine.importSave(new Uint8Array(message.bytes));
      publishSaveFiles("saveFiles", message.requestId);
    } else if (message.type === "deleteSave") {
      machine.deleteSave(message.path);
      publishSaveFiles("saveFiles", message.requestId);
    }
  } catch (error) {
    const fatal = !["importSave", "deleteSave"].includes(message.type);
    failed ||= fatal;
    if (fatal) disconnectRenderer();
    reply({
      type: "error",
      fatal,
      operation: message.type,
      requestId: message.requestId,
      message: error instanceof Error ? error.message : String(error),
    });
  } finally {
    if (message.type !== "boot" && message.generation === generation &&
        Number.isSafeInteger(message.commandSequence)) {
      reply({ type: "commandAck", commandSequence: message.commandSequence });
    }
  }
};
