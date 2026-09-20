let machine = null;
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
      const bindings = await import(message.moduleUrl);
      await bindings.default(message.wasmUrl);
      machine = await bindings.WorkerMachine.create(
        new Uint8Array(message.gameBytes),
        message.config,
      );
      self.postMessage({ type: "ready", files: machine.saveFiles() });
      return;
    }
    if (!machine) return;

    if (message.type === "frame") {
      const result = machine.runFrame(message.queuedAudioSamples ?? -1, !!message.debug, message.outputScale ?? 1);
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
      const transfer = [];
      if (result.frame) transfer.push(result.frame.buffer);
      if (result.audio) transfer.push(result.audio.buffer);
      if (result.gpuFrame) {
        for (const texture of result.gpuFrame.textures) transfer.push(texture.rgba.buffer);
        for (const draw of result.gpuFrame.draws) transfer.push(draw.vertices.buffer);
      }
      self.postMessage({ type: "frame", ...result }, transfer);
      return;
    }
    if (message.type === "enableGpu") machine.setGpuRendererEnabled(!!message.enabled);
    if (message.type === "mouseDown") press("mouse", () => machine.mouseDown(message.v, message.h));
    else if (message.type === "mouseUp") release("mouse", () => machine.mouseUp(message.v, message.h));
    else if (message.type === "mouseMove") machine.mouseMove(message.v, message.h);
    else if (message.type === "keyDown") press(message.macKey, () => machine.keyDown(message.macKey, message.charCode));
    else if (message.type === "keyUp") release(message.macKey, () => machine.keyUp(message.macKey, message.charCode));
    else if (message.type === "importSave") {
      const files = machine.importSave(new Uint8Array(message.bytes));
      self.postMessage({ type: "saveFiles", files });
    } else if (message.type === "deleteSave") {
      self.postMessage({ type: "saveFiles", files: machine.deleteSave(message.path) });
    }
  } catch (error) {
    self.postMessage({
      type: "error",
      message: error instanceof Error ? error.message : String(error),
    });
  }
};
