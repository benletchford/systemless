// One renderer submission in the browser's message queue and one newest owned
// pending complete frame. This transport never coalesces incremental packets.
export class RendererTransport {
  constructor(endpoint, identity, { onFailure, onSubmitted, now = () => performance.now() }) {
    this.endpoint = endpoint;
    this.identity = { generation: identity.generation, rendererGeneration: identity.rendererGeneration };
    this.onFailure = onFailure;
    this.onSubmitted = onSubmitted;
    this.now = now;
    this.inFlight = null;
    this.pending = null;
    this.recycled = [];
    this.sequence = 0;
    this.closed = false;
  }

  submit(packet) {
    if (this.closed) return false;
    const indexed = packet?.kind === "indexed8";
    if ((!indexed && packet?.kind !== "rgba") || packet.complete !== true
        || (indexed && (!(packet.palette instanceof Uint8Array)
          || !(packet.palette.buffer instanceof ArrayBuffer) || packet.palette.byteLength !== 1024))
        || !(packet.pixels instanceof Uint8Array) || !(packet.pixels.buffer instanceof ArrayBuffer)
        || !packet.pixels.byteLength || !Number.isSafeInteger(packet.sequence) || packet.sequence <= this.sequence) {
      this.fail(new Error("Renderer transport requires ordered complete image packets"));
      return false;
    }
    this.sequence = packet.sequence;
    if (this.inFlight) {
      this.recycle(this.pending?.pixels.buffer);
      this.recycle(this.pending?.palette?.buffer);
      this.recycle(this.pending?.cursor?.pixels?.buffer);
      this.pending = packet;
    } else this.send(packet);
    return !this.closed;
  }

  send(packet) {
    this.inFlight = { sequence: packet.sequence, sentAt: this.now(), kind: packet.kind,
      bytes: packet.pixels.byteLength + (packet.palette?.byteLength || 0) + (packet.cursor?.pixels?.byteLength || 0) };
    try {
      this.endpoint.postMessage({ ...packet, ...this.identity, type: "frame", protocolVersion: 2 },
        [...new Set([packet.pixels.buffer, packet.palette?.buffer, packet.cursor?.pixels?.buffer].filter(Boolean))]);
    } catch (error) {
      // A failed structured clone normally retains ownership. Return the newest
      // available complete frame to fallback; never reboot the execution owner.
      if (packet.pixels.byteLength) this.pending = packet;
      this.fail(error);
    }
  }

  receive(message) {
    if (this.closed || message?.generation !== this.identity.generation
        || message.rendererGeneration !== this.identity.rendererGeneration) return;
    if (message.protocolVersion !== 2) {
      this.fail(new Error("Renderer protocol mismatch"));
      return;
    }
    if (message.type === "error") {
      this.fail(new Error(message.message || "Renderer failed"));
      return;
    }
    if (!this.inFlight || message.sequence !== this.inFlight.sequence
        || (message.type !== "submitted" && message.type !== "dropped")) return;
    const elapsedMs = this.now() - this.inFlight.sentAt;
    const { kind, bytes } = this.inFlight;
    this.inFlight = null;
    this.recycle(message.buffer);
    this.recycle(message.paletteBuffer);
    this.recycle(message.cursorBuffer);
    const next = this.pending;
    this.pending = null;
    if (next && !this.closed) this.send(next);
    // Settle the next credit before notifying the host: a callback can submit
    // another frame synchronously and must not create a second in-flight send.
    if (message.type === "submitted" && !this.closed) this.onSubmitted?.({ sequence: message.sequence, elapsedMs, renderMs: message.renderMs, kind, bytes });
  }

  recycle(buffer) {
    if (buffer instanceof ArrayBuffer && buffer.byteLength && this.recycled.length < 2 && !this.recycled.includes(buffer)) this.recycled.push(buffer);
  }

  takeBuffer(byteLength) {
    const index = this.recycled.findIndex(buffer => buffer.byteLength === byteLength);
    return index < 0 ? null : this.recycled.splice(index, 1)[0];
  }

  fail(error) {
    if (this.closed) return;
    const pending = this.pending;
    this.dispose();
    this.onFailure(error, pending);
  }

  dispose() {
    this.closed = true;
    this.pending = this.inFlight = null;
    this.recycled.length = 0;
  }
}
