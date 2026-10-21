// Owns the WebGPU device for guest-visible QuickDraw and QD3D compute.
// The emulation worker blocks while a request runs; this worker's event loop
// stays free so WebGPU readback can complete. See gpu-compute-bridge.js.
let started = false;

self.onmessage = async ({ data }) => {
  if (data?.type !== "init" || started) return;
  started = true;
  const port = data.port;
  try {
    if (data.protocol !== 1) throw new Error("GPU compute protocol mismatch");
    const bridge = await import(data.bridgeUrl);
    if (bridge.GPU_COMPUTE_PROTOCOL !== data.protocol) throw new Error("GPU compute assets mismatch");
    const bindings = await import(data.moduleUrl);
    await bindings.default(data.wasmUrl);
    const compute = await bindings.GpuComputeWorker.create();
    bridge.serveGpuCompute({ port, control: data.control, execute: request => compute.execute(request) });
    if (data.injectFailureAfterMs >= 0) {
      setTimeout(() => compute.injectFailure("injected WebGPU device loss"), data.injectFailureAfterMs);
    }
    port.postMessage({ type: "ready", adapter: compute.adapter() });
  } catch (error) {
    port.postMessage({ type: "unavailable", message: String(error?.message || error) });
  }
};
