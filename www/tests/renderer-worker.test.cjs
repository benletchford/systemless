const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const read = name => fs.readFileSync(path.join(__dirname, '../src', name), 'utf8');
const identity = { generation: 7, rendererGeneration: 2, protocolVersion: 1 };
const frame = (sequence, fields = {}) => ({ ...identity, type: 'frame', kind: 'rgba', complete: true,
  sequence, guestTick: 100, displayGeneration: 1, width: 2, height: 1,
  pixels: new Uint8Array([sequence, 2, 3, 255, 4, 5, 6, 255]), ...fields });

function renderer(options = {}) {
  const messages = [], paints = [], timers = new Map(), listeners = {};
  let next = 0, closed = false;
  const canvas = { width: 1, height: 1,
    getContext: () => options.unsupported ? null : { putImageData: image => {
      if (options.paintError) throw new Error('paint failed');
      paints.push([...image.data]);
    } }, addEventListener: (name, callback) => { listeners[name] = callback; } };
  const context = vm.createContext({ Uint8Array, Uint8ClampedArray, ArrayBuffer,
    performance: { now: () => 42 },
    ImageData: class { constructor(data, width, height) { this.data = data; this.width = width; this.height = height; } },
    setTimeout: callback => { timers.set(++next, callback); return next; },
    clearTimeout: id => timers.delete(id),
    self: { postMessage: (message, transfer = []) => messages.push(structuredClone(message, { transfer })), close: () => { closed = true; } },
  });
  vm.runInContext(read('renderer-worker.js'), context);
  const send = data => context.self.onmessage({ data });
  send({ ...identity, type: 'init', canvas });
  return { send, messages, paints, timers, listeners, canvas, closed: () => closed,
    flush: () => { const jobs = [...timers.values()]; timers.clear(); jobs.forEach(job => job()); } };
}

function transport(endpoint = { postMessage() {} }) {
  const failures = [], submitted = [];
  const context = vm.createContext({ ArrayBuffer, Uint8Array, performance: { now: () => 50 } });
  vm.runInContext(read('renderer-transport.js').replace('export class', 'class') + '\nthis.Transport = RendererTransport;', context);
  const client = new context.Transport(endpoint, identity, {
    onFailure: (error, pending) => failures.push({ error: error.message, pending }),
    onSubmitted: value => submitted.push(value),
  });
  return { client, failures, submitted };
}

test('complete frames coalesce while guest ticks remain frozen; buffers return by transfer', () => {
  const w = renderer();
  const a = frame(1), b = frame(2);
  w.send(a); w.send(b);
  assert.equal(w.timers.size, 1);
  assert.equal(a.pixels.byteLength, 0);
  w.flush();
  assert.deepEqual(w.paints, [[2, 2, 3, 255, 4, 5, 6, 255]]);
  assert.equal(b.pixels.byteLength, 0);
  assert.deepEqual(w.messages.map(m => m.type), ['ready', 'dropped', 'submitted']);
});

test('stale runtime, renderer and frame identifiers cannot replace a pending image', () => {
  const w = renderer();
  w.send(frame(2));
  w.send(frame(10, { generation: 6 }));
  w.send(frame(11, { rendererGeneration: 1 }));
  w.send(frame(1)); w.flush();
  assert.equal(w.paints[0][0], 2);
});

test('mode changes require coherent generation and dimensions', () => {
  const w = renderer();
  w.send(frame(1)); w.flush();
  w.send(frame(2, { width: 1, height: 2, displayGeneration: 2 })); w.flush();
  assert.equal(w.canvas.width, 1);
  assert.equal(w.canvas.height, 2);
  w.send(frame(3));
  assert.equal(w.messages.at(-1).type, 'error');
});

test('partial QD3D, malformed pixels and unversioned geometry are rejected', () => {
  for (const invalid of [{ kind: 'qd3d' }, { complete: false }, { pixels: new Uint8Array(7) }, { width: 1, height: 2 }]) {
    const w = renderer(); w.send(frame(1)); w.flush();
    w.send(frame(2, invalid));
    assert.equal(w.messages.at(-1).type, 'error');
    assert.equal(w.paints.length, 1);
  }
});

test('unsupported context, context loss and paint errors fail once without a guest restart', () => {
  for (const options of [{ unsupported: true }, { paintError: true }, {}]) {
    const w = renderer(options);
    if (!options.unsupported) {
      w.send(frame(1));
      if (!options.paintError) w.listeners.contextlost({ preventDefault() {} });
      w.flush();
    }
    w.send(frame(2)); w.flush();
    assert.equal(w.messages.filter(m => m.type === 'error').length, 1);
    assert.equal(w.timers.size, 0);
  }
});

test('stop cancels pending work and releases the canvas', () => {
  const w = renderer(); w.send(frame(1)); w.send({ ...identity, type: 'stop' }); w.flush();
  assert.equal(w.closed(), true); assert.equal(w.paints.length, 0); assert.equal(w.canvas.width, 1);
});

test('sender bounds the opaque queue, pending frame and recycled buffers', () => {
  const sent = [];
  const { client, failures } = transport({ postMessage: (m, transfer) => sent.push(structuredClone(m, { transfer })) });
  for (let i = 1; i <= 500; i++) client.submit(frame(i));
  assert.equal(sent.length, 1); assert.equal(client.pending.sequence, 500); assert.equal(client.recycled.length, 2);
  client.receive({ ...identity, type: 'submitted', sequence: 99 });
  assert.equal(sent.length, 1);
  client.receive({ ...identity, type: 'submitted', sequence: 1, buffer: sent[0].pixels.buffer, renderMs: 1 });
  assert.equal(sent.length, 2); assert.equal(sent[1].sequence, 500); assert.equal(client.pending, null);
  assert.equal(failures.length, 0);
});

test('renderer replacement rejects stale acknowledgements and returns newest fallback pixels', () => {
  const { client, failures } = transport();
  client.submit(frame(1)); client.submit(frame(2));
  client.receive({ ...identity, rendererGeneration: 1, type: 'error', message: 'old renderer' });
  assert.equal(failures.length, 0);
  client.receive({ ...identity, type: 'error', message: 'lost context' });
  assert.equal(failures[0].pending.sequence, 2);
  assert.equal(client.closed, true); assert.equal(client.recycled.length, 0);
  client.receive({ ...identity, type: 'error' });
  assert.equal(failures.length, 1);
});

test('transfer failure retains available frame ownership for fallback', () => {
  const { client, failures } = transport({ postMessage() { throw new Error('clone failed'); } });
  const packet = frame(1); assert.equal(client.submit(packet), false);
  assert.equal(failures[0].pending, packet); assert.equal(packet.pixels.byteLength, 8);
});


test('synchronous acknowledgement callbacks cannot exceed one in-flight submission', () => {
  const sent = [];
  const { client } = transport({ postMessage: message => sent.push(message.sequence) });
  client.onSubmitted = () => client.submit(frame(3));
  client.submit(frame(1)); client.submit(frame(2));
  client.receive({ ...identity, type: 'submitted', sequence: 1 });
  assert.deepEqual(sent, [1, 2]);
  assert.equal(client.inFlight.sequence, 2);
  assert.equal(client.pending.sequence, 3);
});

test('malformed transport pixels fail through the recovery callback', () => {
  const { client, failures } = transport();
  assert.equal(client.submit(frame(1, { pixels: null })), false);
  assert.equal(failures.length, 1);
  assert.equal(client.closed, true);
});
