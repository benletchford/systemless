// Synchronous guest-visible GPU compute across workers.
//
// The emulation worker must commit QuickDraw/QD3D results to guest memory
// before guest execution resumes, but WebGPU only completes readback on the
// event loop of the thread that owns the device. The compute worker owns the
// device; the emulation worker posts one request and blocks in Atomics.wait
// until the reply is published in shared memory. Requires cross-origin
// isolation for SharedArrayBuffer.
export const GPU_COMPUTE_PROTOCOL = 1;

// Int32 control words. The reply length and status are written before the
// sequence number is stored, so a matching sequence publishes a full reply.
const SEQUENCE = 0, LENGTH = 1, STATUS = 2;
export const CONTROL_BYTES = 16;
export const STATUS_OK = 1, STATUS_FAILED = 2;
const MIN_REPLY_BYTES = 64 * 1024;
// Never resize beyond the largest WebGPU readback the kernels accept.
const MAX_REPLY_BYTES = 1 << 30;

export function availability(scope) {
  if (!scope.crossOriginIsolated) return "page is not cross-origin isolated";
  if (typeof SharedArrayBuffer !== "function" || typeof Atomics?.wait !== "function") {
    return "shared memory is unavailable";
  }
  if (!scope.navigator?.gpu) return "WebGPU is unavailable";
  return null;
}

function replyCapacity(bytes) {
  let capacity = MIN_REPLY_BYTES;
  while (capacity < bytes) capacity *= 2;
  return capacity;
}

// Emulation side. `call` blocks, and throws once the link has failed; a late
// reply after a timeout must never be attributed to a later request.
export class GpuComputeLink {
  constructor({ port, control, timeoutMs = 5000, now = () => performance.now(), onFailure = () => {} }) {
    this.port = port;
    this.control = new Int32Array(control);
    this.timeoutMs = timeoutMs;
    this.now = now;
    this.onFailure = onFailure;
    this.reply = null;
    this.sequence = 0;
    this.ready = false;
    this.failure = null;
    this.calls = 0;
    // Blocking time observed by the guest, including transport and readback.
    this.totalMs = 0;
    this.maxMs = 0;
  }

  timing() {
    return { calls: this.calls, totalMs: this.totalMs, maxMs: this.maxMs };
  }

  fail(reason) {
    if (this.failure) return;
    this.failure = String(reason || "GPU compute failed");
    this.ready = false;
    try { this.port.close(); } catch {}
    this.onFailure(this.failure);
  }

  call(request, capacity) {
    if (this.failure) throw new Error(this.failure);
    if (!this.ready) throw new Error("GPU compute worker is not ready");
    if (!(request instanceof Uint8Array) || !Number.isSafeInteger(capacity) || capacity < 1
        || capacity > MAX_REPLY_BYTES) {
      throw new Error("Invalid GPU compute request");
    }
    let replacement;
    if (!this.reply || this.reply.byteLength < capacity) {
      replacement = this.reply = new SharedArrayBuffer(replyCapacity(capacity));
    }
    const sequence = this.sequence = (this.sequence + 1) | 0 || 1;
    const owned = request.byteOffset === 0 && request.byteLength === request.buffer.byteLength
      ? request : request.slice();
    this.port.postMessage({ type: "request", sequence, request: owned.buffer, reply: replacement },
      [owned.buffer]);
    this.calls++;
    const started = this.now();
    const deadline = started + this.timeoutMs;
    for (;;) {
      const observed = Atomics.load(this.control, SEQUENCE);
      if (observed === sequence) break;
      const remaining = deadline - this.now();
      if (remaining <= 0) {
        this.fail(`GPU compute timed out after ${this.timeoutMs} ms`);
        throw new Error(this.failure);
      }
      Atomics.wait(this.control, SEQUENCE, observed, remaining);
    }
    const elapsed = this.now() - started;
    this.totalMs += elapsed;
    this.maxMs = Math.max(this.maxMs, elapsed);
    const length = Atomics.load(this.control, LENGTH);
    if (Atomics.load(this.control, STATUS) !== STATUS_OK || length < 0 || length > this.reply.byteLength) {
      this.fail("GPU compute worker could not deliver a reply");
      throw new Error(this.failure);
    }
    return new Uint8Array(this.reply, 0, length).slice();
  }
}

// Compute side. Requests run strictly one at a time because the kernels
// reuse scratch buffers; each reply is published by its sequence number.
export function serveGpuCompute({ port, control, execute }) {
  const words = new Int32Array(control);
  let reply = null;
  let chain = Promise.resolve();
  const publish = (sequence, status, length) => {
    Atomics.store(words, LENGTH, length);
    Atomics.store(words, STATUS, status);
    Atomics.store(words, SEQUENCE, sequence);
    Atomics.notify(words, SEQUENCE);
  };
  const handle = async ({ sequence, request, reply: replacement }) => {
    if (replacement) reply = new Uint8Array(replacement);
    let bytes = null;
    try { bytes = await execute(new Uint8Array(request)); } catch {}
    if (!(bytes instanceof Uint8Array) || !reply || bytes.byteLength > reply.byteLength) {
      publish(sequence, STATUS_FAILED, 0);
      return;
    }
    reply.set(bytes);
    publish(sequence, STATUS_OK, bytes.byteLength);
  };
  port.onmessage = ({ data }) => {
    if (data?.type !== "request" || !Number.isSafeInteger(data.sequence)) return;
    chain = chain.then(() => handle(data));
  };
  return () => chain;
}
