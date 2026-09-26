import { RendererTransport } from "./renderer-transport.js";

export const DIRECT_RENDERER_PROTOCOL = 1;

// Runs on the execution owner. Only completed owned images cross this port;
// guest execution, display demand and ordered input are unchanged.
export class RendererOwner {
  constructor(port, identity, { sequence = 0, displayGeneration = 0, notify }) {
    this.port = port;
    this.identity = identity;
    this.sequence = sequence;
    this.displayGeneration = displayGeneration;
    this.layout = null;
    this.closed = false;
    this.notify = message => notify({ ...message, ...identity, type: "rendererStatus" });
    this.transport = new RendererTransport({ postMessage: (message, transfer) => {
      port.postMessage(message, transfer);
      this.notify({ event: "queued", sequence: message.sequence });
    } }, identity, {
      onFailure: error => this.fail(error),
      onSubmitted: metrics => this.notify({ event: "submitted", ...metrics }),
    });
    port.onmessage = ({ data }) => this.transport.receive(data);
    port.onmessageerror = () => this.fail(new Error("Unreadable direct renderer reply"));
    port.start();
  }

  submit(result) {
    if (this.closed) return false;
    const frame = result.compactFrame || result.indexedFrame || (result.frame && {
      kind: "rgba", complete: true, width: result.width, height: result.height, pixels: result.frame,
    });
    if (!frame) return false;
    const layout = `${frame.width}:${frame.height}:${frame.kind}:${frame.stride ?? 0}:`
      + (frame.compact ? `${frame.compact.width}:${frame.compact.height}:${frame.compact.scale}` : "");
    if (this.layout !== layout) {
      this.layout = layout;
      this.displayGeneration++;
    }
    const sequence = ++this.sequence;
    const bytes = [frame.pixels, frame.palette, frame.cursor?.pixels, frame.compact?.cells, frame.compact?.detail]
      .filter(Boolean).reduce((total, view) => total + view.byteLength, 0);
    // Remove transferred or queued image ownership from the host reply. Audio,
    // progress and save acknowledgements still follow their existing route.
    delete result.frame;
    delete result.indexedFrame;
    delete result.compactFrame;
    result.directFrame = { width: frame.width, height: frame.height, sequence, rendererGeneration: this.identity.rendererGeneration, kind: frame.kind, bytes };
    this.transport.submit({ ...frame, outputScale: result.outputScale ?? 1, sequence, displayGeneration: this.displayGeneration });
    return true;
  }

  fail(error) {
    if (this.closed) return;
    this.notify({ event: "error", message: String(error?.message || error) });
    this.dispose();
  }

  dispose() {
    if (this.closed) return;
    this.closed = true;
    this.transport.dispose();
    this.port.onmessage = this.port.onmessageerror = null;
    this.port.close();
  }
}
