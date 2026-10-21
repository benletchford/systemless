const { test } = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { Worker, MessageChannel } = require('node:worker_threads');

const bridgeUrl = pathToFileURL(path.join(__dirname, '../src/gpu-compute-bridge.js')).href;
const load = () => import(bridgeUrl);

// A real second thread runs the compute side, so the emulation side can block
// in Atomics.wait exactly as it does in the browser.
function computeThread(control, behaviour) {
  const channel = new MessageChannel();
  const worker = new Worker(`
    const { workerData } = require('node:worker_threads');
    import(workerData.bridgeUrl).then(({ serveGpuCompute }) => {
      const behaviour = workerData.behaviour;
      let calls = 0;
      serveGpuCompute({ port: workerData.port, control: workerData.control, execute: async request => {
        calls++;
        if (behaviour === 'hang') return new Promise(() => {});
        if (behaviour === 'oversized') return new Uint8Array(1 << 20);
        if (behaviour === 'throws') throw new Error('kernel failed');
        if (behaviour === 'slow-first' && calls === 1) await new Promise(r => setTimeout(r, 50));
        // Reply: call number followed by the reversed request bytes.
        return Uint8Array.from([calls, ...Array.from(request).reverse()]);
      } });
      workerData.port.postMessage({ type: 'ready' });
    });
  `, { eval: true, workerData: { bridgeUrl, control, behaviour, port: channel.port2 }, transferList: [channel.port2] });
  return { worker, port: channel.port1 };
}

async function connect(behaviour, options = {}) {
  const bridge = await load();
  const control = new SharedArrayBuffer(bridge.CONTROL_BYTES);
  const { worker, port } = computeThread(control, behaviour);
  await new Promise(resolve => port.once('message', resolve));
  const failures = [];
  const link = new bridge.GpuComputeLink({ port, control, onFailure: reason => failures.push(reason), ...options });
  link.ready = true;
  return { link, worker, failures, close: () => worker.terminate() };
}

test('blocking calls return each reply in order and grow the shared reply buffer', async () => {
  const { link, failures, close } = await connect('slow-first');
  try {
    assert.deepEqual([...link.call(Uint8Array.from([1, 2, 3]), 16)], [1, 3, 2, 1]);
    const first = link.reply;
    assert.equal(first.byteLength, 64 * 1024);
    // A view into a larger buffer must still be sent intact.
    const backing = Uint8Array.from([9, 8, 7, 6]);
    assert.deepEqual([...link.call(backing.subarray(1, 3), 16)], [2, 7, 8]);
    assert.equal(link.reply, first, 'reply buffer is reused while large enough');
    assert.deepEqual([...link.call(Uint8Array.of(5), 200_000)], [3, 5]);
    assert.equal(link.reply.byteLength, 256 * 1024);
    assert.equal(link.calls, 3);
    const timing = link.timing();
    assert.equal(timing.calls, 3);
    assert.ok(timing.maxMs >= 50, 'the slow first reply is the maximum');
    assert.ok(timing.totalMs >= timing.maxMs);
    assert.deepEqual(failures, []);
  } finally { await close(); }
});

test('a stalled device times out once and every later call stays on the CPU', async () => {
  const { link, failures, close } = await connect('hang', { timeoutMs: 150 });
  try {
    const started = Date.now();
    assert.throws(() => link.call(Uint8Array.of(1), 16), /timed out after 150 ms/);
    assert.ok(Date.now() - started >= 140);
    assert.throws(() => link.call(Uint8Array.of(1), 16), /timed out/);
    assert.equal(link.calls, 1, 'no request is posted after failure');
    assert.equal(failures.length, 1);
  } finally { await close(); }
});

test('kernel exceptions and replies larger than the reserved capacity fail the link', async () => {
  for (const behaviour of ['throws', 'oversized']) {
    const { link, failures, close } = await connect(behaviour);
    try {
      assert.throws(() => link.call(Uint8Array.of(1), 16), /could not deliver a reply/);
      assert.equal(failures.length, 1);
    } finally { await close(); }
  }
});

test('calls before readiness or with invalid capacity never block', async () => {
  const bridge = await load();
  const posted = [];
  const link = new bridge.GpuComputeLink({ port: { postMessage: m => posted.push(m), close() {} },
    control: new SharedArrayBuffer(bridge.CONTROL_BYTES) });
  assert.throws(() => link.call(Uint8Array.of(1), 16), /not ready/);
  link.ready = true;
  assert.throws(() => link.call(Uint8Array.of(1), 0), /Invalid/);
  assert.throws(() => link.call([1], 16), /Invalid/);
  assert.deepEqual(posted, []);
});

test('availability requires isolation, shared memory and WebGPU', async () => {
  const { availability } = await load();
  assert.match(availability({ crossOriginIsolated: false, navigator: { gpu: {} } }), /cross-origin isolated/);
  assert.match(availability({ crossOriginIsolated: true, navigator: {} }), /WebGPU/);
  assert.equal(availability({ crossOriginIsolated: true, navigator: { gpu: {} } }), null);
});
