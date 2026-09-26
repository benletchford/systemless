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
