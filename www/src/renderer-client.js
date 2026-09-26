import { RendererTransport } from "./renderer-transport.js";

let nextRendererGeneration = 0;

// Keep the logical input canvas and its listeners in place. Only this sibling
// display canvas is transferred, so presenter replacement cannot lose a held
// key, pointer capture, focus, Leptos node reference or touch-release callback.
export class RendererClient {
  constructor(logicalCanvas, workerUrl, generation) {
    this.logicalCanvas = logicalCanvas;
    this.identity = { generation, rendererGeneration: ++nextRendererGeneration, protocolVersion: 1 };
    this.phase = "booting";
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
        onSubmitted: metrics => {
          this.submitted = true;
          this.canvas.style.visibility = "visible";
          logicalCanvas.setAttribute("data-render-sequence", String(metrics.sequence));
          logicalCanvas.setAttribute("data-render-roundtrip-ms", String(metrics.elapsedMs));
          logicalCanvas.setAttribute("data-render-submit-ms", String(metrics.renderMs));
        },
      });
      this.worker.onmessage = ({ data }) => this.receive(data);
      this.worker.onerror = event => { event.preventDefault(); this.fail(new Error(event.message || "Renderer worker crashed")); };
      this.worker.onmessageerror = () => this.fail(new Error("Unreadable renderer reply"));
      this.worker.postMessage({ ...this.identity, type: "init", canvas: offscreen }, [offscreen]);
      this.timer = setInterval(() => this.checkTimeout(), 500);
    } catch (error) {
      this.fail(error);
    }
  }

  receive(message) {
    if (this.phase === "failed" || this.phase === "disposed"
        || message?.generation !== this.identity.generation
        || message.rendererGeneration !== this.identity.rendererGeneration) return;
    if (message.protocolVersion !== 1) return this.fail(new Error("Renderer protocol mismatch"));
    if (message.type === "ready") {
      if (this.phase !== "booting" || !message.kinds?.includes("rgba")) {
        return this.fail(new Error("Invalid renderer capability reply"));
      }
      this.phase = "ready";
      this.waitingMs = 0;
      const first = this.bootFrame;
      this.bootFrame = null;
      if (first) this.transport.submit(first);
    } else this.transport.receive(message);
  }

  paint(width, height, pixels) {
    if (this.phase === "failed" || this.phase === "disposed") return false;
    if (!this.dimensions || width !== this.dimensions[0] || height !== this.dimensions[1]) {
      this.displayGeneration++;
      this.dimensions = [width, height];
      this.syncGeometry();
    }
    const packet = { kind: "rgba", complete: true, sequence: ++this.sequence,
      displayGeneration: this.displayGeneration, width, height, pixels };
    if (this.phase === "booting") this.bootFrame = packet;
    else this.transport.submit(packet);
    return this.phase !== "failed";
  }

  checkTimeout() {
    const now = performance.now();
    const elapsed = Math.max(0, now - this.lastCheck);
    this.lastCheck = now;
    if (document.visibilityState === "hidden") return;
    const sequence = this.phase === "booting" ? "boot" : this.transport?.inFlight?.sequence;
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
    return { phase: this.phase, error: this.error, submitted: this.submitted,
      pending: !!(this.bootFrame || this.transport?.inFlight || this.transport?.pending) };
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
