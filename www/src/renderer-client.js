import { RendererTransport } from "./renderer-transport.js";

let nextRendererGeneration = 0;

// Keep the logical input canvas and its listeners in place. Only this sibling
// display canvas is transferred, so presenter replacement cannot lose a held
// key, pointer capture, focus, Leptos node reference or touch-release callback.
export class RendererClient {
  constructor(logicalCanvas, workerUrl, generation, { backend = "canvas2d", owner = null, direct = false } = {}) {
    this.logicalCanvas = logicalCanvas;
    this.owner = owner;
    this.directWanted = direct && !!owner;
    this.direct = false;
    this.directNeedsSnapshot = false;
    this.directInFlight = null;
    this.lastSubmittedSequence = 0;
    this.onOwnerStatus = ({ data }) => {
      if (!this.direct || this.phase !== "ready" || data?.type !== "rendererStatus"
          || data.generation !== this.identity.generation
          || data.rendererGeneration !== this.identity.rendererGeneration) return;
      if (data.event === "error") this.fail(new Error(data.message));
      else if (data.event === "queued" && data.sequence > this.lastSubmittedSequence) this.directInFlight = data.sequence;
      else if (data.event === "submitted") {
        if (this.directInFlight === data.sequence) this.directInFlight = null;
        this.logicalCanvas.setAttribute("data-render-credit-return-ms", String(data.elapsedMs));
      }
    };
    this.identity = { generation, rendererGeneration: ++nextRendererGeneration, protocolVersion: 4 };
    this.phase = "booting";
    this.backend = null;
    this.kinds = ["rgba"];
    this.error = null;
    this.submitted = false;
    this.sequence = 0;
    this.displayGeneration = 0;
    this.dimensions = null;
    this.bootFrame = null;
    this.recoveryFrame = null;
    this.worker = this.canvas = this.observer = this.timer = this.transport = null;
    this.syncGeometry = () => {
      if (!this.canvas) return;
      const source = this.logicalCanvas;
      const parent = source.parentElement;
      const bounds = source.getBoundingClientRect();
      const container = parent.getBoundingClientRect();
      // offsetWidth/offsetLeft round fractional CSS pixels. That changes pixel
      // sampling at fractional display scales even when the packet is exact.
      Object.assign(this.canvas.style, {
        left: `${bounds.left - container.left - parent.clientLeft + parent.scrollLeft}px`,
        top: `${bounds.top - container.top - parent.clientTop + parent.scrollTop}px`,
        width: `${bounds.width}px`, height: `${bounds.height}px`,
      });
    };
    this.lastCheck = performance.now();
    this.waitingSequence = null;
    this.waitingMs = 0;
    try {
      if (!logicalCanvas.parentElement || typeof logicalCanvas.transferControlToOffscreen !== "function"
          || typeof ResizeObserver !== "function") throw new Error("Offscreen presentation unavailable");
      const canvas = document.createElement("canvas");
      this.canvas = canvas;
      canvas.width = logicalCanvas.width;
      canvas.height = logicalCanvas.height;
      canvas.setAttribute("aria-hidden", "true");
      canvas.setAttribute("data-presentation-canvas", "worker");
      Object.assign(canvas.style, { position: "absolute", pointerEvents: "none", imageRendering: "pixelated",
        display: "block", visibility: "hidden" });
      logicalCanvas.parentElement.insertBefore(canvas, logicalCanvas.nextSibling);
      this.syncGeometry();
      this.observer = new ResizeObserver(this.syncGeometry);
      this.observer.observe(logicalCanvas);
      this.observer.observe(logicalCanvas.parentElement);
      window.addEventListener("resize", this.syncGeometry);
      // No context may have been created on this new canvas before transfer.
      const offscreen = canvas.transferControlToOffscreen();
      this.worker = new Worker(workerUrl);
      this.transport = new RendererTransport(this.worker, this.identity, {
        onFailure: (error, pending) => this.fail(error, pending),
        onSubmitted: metrics => this.markSubmitted(metrics),
      });
      this.worker.onmessage = ({ data }) => this.receive(data);
      this.worker.onerror = event => { event.preventDefault(); this.fail(new Error(event.message || "Renderer worker crashed")); };
      this.worker.onmessageerror = () => this.fail(new Error("Unreadable renderer reply"));
      this.worker.postMessage({ ...this.identity, type: "init", canvas: offscreen, backend }, [offscreen]);
      this.timer = setInterval(() => this.checkTimeout(), 500);
    } catch (error) {
      this.fail(error);
    }
  }

  receive(message) {
    if (this.phase === "failed" || this.phase === "disposed"
        || message?.generation !== this.identity.generation
        || message.rendererGeneration !== this.identity.rendererGeneration) return;
    if (message.protocolVersion !== 4) return this.fail(new Error("Renderer protocol mismatch"));
    if (message.type === "ready") {
      if (this.phase !== "booting" || !message.kinds?.includes("rgba")) {
        return this.fail(new Error("Invalid renderer capability reply"));
      }
      this.backend = message.backend;
      this.kinds = message.kinds;
      this.phase = "ready";
      this.waitingMs = 0;
      const first = this.bootFrame;
      this.bootFrame = null;
      if (this.directWanted && typeof MessageChannel === "function") {
        try { this.startDirect(); } catch (error) { this.fail(error); }
      } else if (first) this.transport.submit(first);
    } else if (message.type === "directSubmitted" && this.direct && this.phase === "ready") {
      if (message.sequence <= this.lastSubmittedSequence) return;
      if (this.directInFlight !== null && this.directInFlight <= message.sequence) this.directInFlight = null;
      this.directNeedsSnapshot = false;
      // Layout/input geometry follows the submitted image, not newer owner
      // metadata which may be replaced while the renderer is still busy.
      const { width, height, outputScale } = message;
      if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height)
          || width < 1 || height < 1 || !Number.isSafeInteger(outputScale) || outputScale < 1 || outputScale > 4) {
        this.fail(new Error("Invalid direct display metadata")); return;
      }
      const canvas = this.logicalCanvas;
      if (canvas.width !== width) canvas.width = width;
      if (canvas.height !== height) canvas.height = height;
      canvas.setAttribute("data-output-scale", String(outputScale));
      for (const element of [canvas, canvas.parentElement]) {
        element.style.setProperty("--game-aspect-ratio", `${width} / ${height}`);
        element.style.setProperty("--game-aspect-width", String(width));
        element.style.setProperty("--game-aspect-height", String(height));
      }
      this.syncGeometry();
      this.markSubmitted(message);
    } else this.transport.receive(message);
  }

  markSubmitted(metrics) {
    this.submitted = true;
    this.lastSubmittedSequence = metrics.sequence;
    this.canvas.style.visibility = "visible";
    const canvas = this.logicalCanvas;
    canvas.setAttribute("data-render-sequence", String(metrics.sequence));
    canvas.setAttribute("data-render-packet-kind", metrics.kind);
    canvas.setAttribute("data-render-packet-bytes", String(metrics.bytes));
    if (Number.isFinite(metrics.elapsedMs)) canvas.setAttribute("data-render-roundtrip-ms", String(metrics.elapsedMs));
    canvas.setAttribute("data-render-submit-ms", String(metrics.renderMs));
  }

  startDirect() {
    const channel = new MessageChannel();
    this.direct = true;
    this.directNeedsSnapshot = true;
    this.owner.addEventListener("message", this.onOwnerStatus);
    try {
      this.worker.postMessage({ ...this.identity, type: "connectOwner", port: channel.port1 }, [channel.port1]);
      this.owner.postMessage({ type: "connectRenderer", generation: this.identity.generation,
        rendererGeneration: this.identity.rendererGeneration, rendererProtocol: 4,
        sequence: this.sequence, displayGeneration: this.displayGeneration,
        port: channel.port2 }, [channel.port2]);
      this.logicalCanvas.setAttribute("data-render-transport", "direct");
    } catch (error) {
      channel.port1.close(); channel.port2.close();
      throw error;
    }
  }

  paint(width, height, pixels) {
    return this.paintPacket({ kind: "rgba", complete: true, width, height, pixels });
  }

  paintPacket(frame) {
    // Pre-handoff host replies can arrive after the port is installed. A forced
    // fresh owner snapshot replaces them; never race two presentation senders.
    if (this.direct) return this.phase === "ready";
    const { width, height } = frame;
    if (!this.kinds.includes(frame.kind)) { this.fail(new Error("Unsupported renderer packet kind")); return false; }
    if (this.phase === "failed" || this.phase === "disposed") return false;
    const layout = frame.kind === "compact" ? `compact:${frame.compact.width}:${frame.compact.height}:${frame.compact.scale}` : `${frame.kind}:${frame.kind === "indexed8" ? frame.stride : width * 4}`;
    if (!this.dimensions || width !== this.dimensions[0] || height !== this.dimensions[1] || layout !== this.dimensions[2]) {
      this.displayGeneration++;
      this.dimensions = [width, height, layout];
      this.syncGeometry();
    }
    const packet = { ...frame, sequence: ++this.sequence, displayGeneration: this.displayGeneration };
    if (this.phase === "booting") this.bootFrame = packet;
    else this.transport.submit(packet);
    return this.phase !== "failed";
  }

  checkTimeout() {
    const now = performance.now();
    const elapsed = Math.max(0, now - this.lastCheck);
    this.lastCheck = now;
    if (document.visibilityState === "hidden") return;
    const sequence = this.phase === "booting" ? "boot"
      : this.direct ? (this.directInFlight ?? (this.directNeedsSnapshot ? "handoff" : null))
      : this.transport?.inFlight?.sequence;
    if (sequence === undefined || sequence === null) {
      this.waitingSequence = null;
      this.waitingMs = 0;
      return;
    }
    if (sequence !== this.waitingSequence) {
      this.waitingSequence = sequence;
      this.waitingMs = 0;
    } else this.waitingMs += elapsed;
    if (this.waitingMs >= 10000) this.fail(new Error(this.phase === "booting"
      ? "Renderer startup timed out" : "Renderer submission timed out"));
  }

  status() {
    return { phase: this.phase, backend: this.backend, kinds: this.kinds, error: this.error, submitted: this.submitted,
      direct: this.direct, needsSnapshot: this.directNeedsSnapshot,
      pending: !!(this.bootFrame || this.transport?.inFlight || this.transport?.pending
        || this.directInFlight || this.directNeedsSnapshot) };
  }

  takeRecoveryFrame() {
    const frame = this.recoveryFrame;
    this.recoveryFrame = null;
    return frame;
  }

  fail(error, pending) {
    if (this.phase === "failed" || this.phase === "disposed") return;
    const recovery = pending || this.transport?.pending || this.bootFrame;
    this.phase = "failed";
    this.error = String(error?.message || error);
    this.release();
    this.recoveryFrame = recovery;
    this.logicalCanvas.setAttribute("data-render-fallback", this.error);
    // If recovery is absent, the host must request a fresh owner snapshot.
    // The in-flight transfer cannot be recovered from a dead renderer.
  }

  release() {
    if (this.owner && this.direct) {
      this.owner.removeEventListener("message", this.onOwnerStatus);
      try { this.owner.postMessage({ type: "disconnectRenderer", generation: this.identity.generation,
        rendererGeneration: this.identity.rendererGeneration }); } catch (_) { /* owner already stopped */ }
    }
    this.owner = null;
    this.direct = this.directNeedsSnapshot = false;
    this.directInFlight = null;
    if (this.timer !== null) clearInterval(this.timer);
    this.timer = null;
    this.observer?.disconnect();
    this.observer = null;
    window.removeEventListener("resize", this.syncGeometry);
    if (this.worker) {
      this.worker.onmessage = this.worker.onerror = this.worker.onmessageerror = null;
      this.worker.terminate();
    }
    this.worker = null;
    this.transport?.dispose();
    this.transport = null;
    this.bootFrame = null;
    this.canvas?.remove();
    this.canvas = null;
  }

  dispose() {
    this.phase = "disposed";
    this.release();
    this.recoveryFrame = null;
  }
}
