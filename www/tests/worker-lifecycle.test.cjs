const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function bridge() {
  const source = fs.readFileSync(path.join(__dirname, '../src/browser_bridge.rs'), 'utf8');
  const js = source.match(/inline_js = r#"([\s\S]*?)"#\)\]/)[1].replace(/export /g, '');
  const timers = new Map();
  let id = 0;
  const context = vm.createContext({
    setTimeout(callback) { timers.set(++id, callback); return id; },
    clearTimeout(id) { timers.delete(id); },
  });
  vm.runInContext(js, context);
  const worker = {
    sent: [], terminated: false,
    postMessage(...args) { this.sent.push(args); },
    terminate() { this.terminated = true; },
  };
  const progress = [];
  const message = { type: 'boot', generation: 7, protocolVersion: 1 };
  const promise = context.bootSystemlessWorker(worker, message, [], 100, p => progress.push(p));
  const receive = data => worker.onmessage?.({ data: { generation: 7, protocolVersion: 1, ...data } });
  return { context, worker, timers, promise, progress, receive };
}

test('boot ignores stale generations, forwards progress and settles once', async () => {
  const b = bridge();
  b.receive({ type: 'ready', generation: 6 });
  assert.equal(b.timers.size, 1);
  b.receive({ type: 'progress', progress: 'mounting' });
  assert.deepEqual(b.progress, ['mounting']);
  b.receive({ type: 'ready' });
  assert.equal((await b.promise).generation, 7);
  assert.equal(b.timers.size, 0);
  assert.equal(b.worker.onmessage, null);
  assert.equal(b.worker.onerror, null);
  assert.equal(b.worker.onmessageerror, null);
});

test('navigation cancellation terminates startup and removes listeners', async () => {
  const b = bridge();
  b.context.cancelSystemlessWorker(b.worker);
  await assert.rejects(b.promise, /cancelled/);
  assert.equal(b.worker.terminated, true);
  assert.equal(b.timers.size, 0);
  assert.equal(b.worker.onmessage, null);
});

for (const [name, fail, expected] of [
  ['timeout', b => [...b.timers.values()][0](), /timed out/],
  ['crash', b => b.worker.onerror({ message: 'crashed' }), /crashed/],
  ['unreadable reply', b => b.worker.onmessageerror(), /unreadable/],
  ['stale protocol', b => b.receive({ type: 'ready', protocolVersion: 2 }), /incompatible/],
  ['boot failure', b => b.receive({ type: 'error', message: 'bad archive' }), /bad archive/],
]) {
  test(`boot rejects ${name} and clears resources`, async () => {
    const b = bridge();
    fail(b);
    await assert.rejects(b.promise, expected);
    assert.equal(b.timers.size, 0);
    assert.equal(b.worker.onmessage, null);
  });
}

test('worker startup yields a task instead of an already resolved microtask', async () => {
  const b = bridge();
  b.receive({ type: 'ready' });
  await b.promise;
  let settled = false;
  const yieldPromise = b.context.yieldSystemlessTask().then(() => { settled = true; });
  await Promise.resolve();
  assert.equal(settled, false);
  assert.equal(b.timers.size, 1);
  [...b.timers.values()][0]();
  await yieldPromise;
  assert.equal(settled, true);
});

async function commandBridge() {
  const b = bridge();
  b.receive({ type: 'ready' });
  await b.promise;
  b.worker.sent.length = 0;
  b.post = (type, fields = {}) => b.context.postSystemlessWorkerCommand(
    b.worker, { type, generation: 7, ...fields }, [],
  );
  b.ack = (sequence, generation = 7) => b.context.acknowledgeSystemlessWorkerCommand(b.worker, generation, sequence);
  return b;
}

test('command demand is bounded and movement never coalesces across a transition', async () => {
  const b = await commandBridge();
  for (let i = 0; i < 8; i++) b.post('keyDown', { macKey: i });
  for (let i = 0; i < 10_000; i++) b.post('mouseMove', { h: i, v: i });
  b.post('mouseUp', { h: 50, v: 70 });
  b.post('mouseMove', { h: 10_000, v: 10_000 });
  assert.equal(b.worker.sent.length, 8);
  b.ack(1);
  b.ack(2);
  b.ack(3);
  assert.deepEqual(b.worker.sent.slice(8).map(([m]) => [m.type, m.h, m.v]), [
    ['mouseMove', 9999, 9999], ['mouseUp', 50, 70], ['mouseMove', 10_000, 10_000],
  ]);
});

test('a full transition queue rejects rather than dropping input silently', async () => {
  const b = await commandBridge();
  for (let i = 0; i < 264; i++) b.post(i % 2 ? 'keyUp' : 'keyDown', { index: i });
  assert.throws(() => b.post('keyUp'), /queue is full/);
  assert.equal(b.worker.sent.length, 8);
  for (let sequence = 1; sequence <= 264; sequence++) b.ack(sequence);
  assert.deepEqual(b.worker.sent.map(([m]) => m.index), Array.from({ length: 264 }, (_, i) => i));
});

test('stale and duplicate acknowledgements cannot release command credit', async () => {
  const b = await commandBridge();
  for (let i = 0; i < 10; i++) b.post('keyDown');
  b.ack(1, 6);
  b.ack(999);
  assert.equal(b.worker.sent.length, 8);
  b.ack(1);
  b.ack(1);
  assert.equal(b.worker.sent.length, 9);
  b.context.cancelSystemlessWorker(b.worker);
  b.ack(2);
  assert.equal(b.worker.sent.length, 9);
});

test('shutdown drains ordered commands and waits for the save completion', async () => {
  const b = await commandBridge();
  for (let i = 0; i < 264; i++) b.post('keyDown', { index: i });
  let completed = false;
  const shutdown = b.context.shutdownSystemlessWorker(b.worker, 7).then(() => { completed = true; });
  assert.throws(() => b.post('mouseUp'), /shutting down/);
  assert.equal(b.worker.terminated, false);
  for (let sequence = 1; sequence <= 264; sequence++) {
    b.receive({ type: 'commandAck', commandSequence: sequence });
  }
  assert.equal(b.worker.sent.at(-1)[0].type, 'shutdown');
  b.receive({ type: 'stopped', generation: 6 });
  await Promise.resolve();
  assert.equal(completed, false);
  b.receive({ type: 'stopped' });
  await shutdown;
  assert.equal(completed, true);
  assert.equal(b.worker.terminated, true);
  assert.equal(b.timers.size, 0);
});

for (const [name, fail] of [
  ['timeout', b => [...b.timers.values()][0]()],
  ['crash', b => b.worker.onerror({ message: 'storage failed' })],
  ['save failure', b => b.receive({ type: 'error', message: 'write aborted' })],
]) {
  test(`shutdown exposes ${name} and releases its resources`, async () => {
    const b = await commandBridge();
    const shutdown = b.context.shutdownSystemlessWorker(b.worker, 7);
    fail(b);
    await assert.rejects(shutdown);
    assert.equal(b.worker.terminated, true);
    assert.equal(b.worker.onmessage, null);
    assert.equal(b.timers.size, 0);
  });
}

test('restarting the same game waits for the old owner while other games may start', async () => {
  const b = await commandBridge();
  const shutdown = b.context.shutdownSystemlessWorker(b.worker, 7, 'game-one');
  let sameGameReady = false;
  let otherGameReady = false;
  const same = b.context.waitSystemlessWorkerShutdown('game-one').then(() => { sameGameReady = true; });
  await b.context.waitSystemlessWorkerShutdown('game-two').then(() => { otherGameReady = true; });
  assert.equal(sameGameReady, false);
  assert.equal(otherGameReady, true);
  b.receive({ type: 'stopped' });
  await Promise.all([shutdown, same]);
  assert.equal(sameGameReady, true);
  await b.context.waitSystemlessWorkerShutdown('game-one');
});

test('a recoverable command error does not interrupt the final save flush', async () => {
  const b = await commandBridge();
  const shutdown = b.context.shutdownSystemlessWorker(b.worker, 7, 'game');
  b.receive({ type: 'error', fatal: false, message: 'invalid import' });
  assert.equal(b.worker.terminated, false);
  b.receive({ type: 'stopped' });
  await assert.rejects(shutdown, /invalid import/);
  assert.equal(b.worker.terminated, true);
});

test('prefetch cache bounds unused downloads and hands ownership to the consumer', async () => {
  const b = bridge();
  b.receive({ type: 'ready' });
  await b.promise;
  const requests = [];
  b.context.AbortController = AbortController;
  b.context.fetch = (url, options) => {
    requests.push({ url, signal: options.signal });
    return new Promise(() => {});
  };
  b.context.prefetchArchive('a');
  b.context.prefetchArchive('b');
  b.context.prefetchArchive('c');
  assert.equal(requests[0].signal.aborted, true);
  assert.equal(b.context.prefetchedArchive('a'), null);
  const active = b.context.prefetchedArchive('b');
  b.context.prefetchArchive('d');
  b.context.prefetchArchive('e');
  assert.equal(requests[1].signal.aborted, false);
  assert.equal(requests[2].signal.aborted, true);
  b.context.cancelArchiveFetch(active);
  assert.equal(requests[1].signal.aborted, true);
  assert.equal(b.context.prefetchedArchive('b'), null);
});

test('cancelled archive worker releases its listeners and object URL exactly once', async () => {
  const b = bridge();
  b.receive({ type: 'ready' });
  await b.promise;
  const revoked = [];
  let worker;
  b.context.Blob = Blob;
  b.context.URL = { createObjectURL: () => 'blob:archive', revokeObjectURL: url => revoked.push(url) };
  b.context.Worker = class {
    constructor() { worker = this; this.terminations = 0; }
    postMessage() {}
    terminate() { this.terminations++; }
  };
  const download = b.context.fetchArchiveInWorker('archive');
  b.context.cancelArchiveFetch(download);
  b.context.cancelArchiveFetch(download);
  await assert.rejects(download, /cancelled/);
  assert.equal(worker.terminations, 1);
  assert.deepEqual(revoked, ['blob:archive']);
  assert.equal(worker.onmessage, null);
  assert.equal(worker.onerror, null);
  assert.equal(worker.onmessageerror, null);
});

test('production worker assets are discovered from hashed resource URLs', async () => {
  const b = bridge();
  b.receive({ type: 'ready' });
  await b.promise;
  b.context.performance = { getEntriesByType: () => [
    { name: 'https://example.test/systemless-org-a123.js' },
    { name: 'https://example.test/systemless-org-a123_bg.wasm?v=1' },
  ] };
  b.context.document = { scripts: [] };
  assert.deepEqual(Array.from(b.context.systemlessRuntimeAssets()), [
    'https://example.test/systemless-org-a123.js',
    'https://example.test/systemless-org-a123_bg.wasm?v=1',
  ]);
});

test('asset discovery uses the module script when its resource entry is unavailable', async () => {
  const b = bridge();
  b.receive({ type: 'ready' });
  await b.promise;
  b.context.performance = { getEntriesByType: () => [{ name: '/systemless-123_bg.wasm' }] };
  b.context.document = { scripts: [{ src: '/systemless-123.js' }] };
  assert.deepEqual(Array.from(b.context.systemlessRuntimeAssets()), ['/systemless-123.js', '/systemless-123_bg.wasm']);
  b.context.performance.getEntriesByType = () => [];
  assert.deepEqual(Array.from(b.context.systemlessRuntimeAssets()), []);
});
