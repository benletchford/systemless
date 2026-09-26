const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');

function worker(steps = () => 1, frameTicks = 6) {
  const keys = new Set();
  const frames = [];
  const messages = [];
  let mouse = false;
  let releasedAt;
  let now = 0;
  let guestTick = 0;
  let running = true;
  let uiTracking = false;
  const stub = {
    keyDown: key => keys.add(key),
    keyUp: key => keys.delete(key),
    mouseDown: () => { mouse = true; },
    mouseUp: (v, h) => { mouse = false; releasedAt = [v, h]; },
    runFrame: () => {
      frames.push({ keys: [...keys], mouse });
      now += 90;
      guestTick = (guestTick + frameTicks) >>> 0;
      return { lastSteps: steps(), guestTick, running, uiTracking };
    },
  };
  const context = vm.createContext({
    self: { postMessage(message) { messages.push(message); } },
    stub,
    performance: { now: () => now },
  });
  vm.runInContext(fs.readFileSync(path.join(__dirname, '../src/emulator-worker.js'), 'utf8'), context);
  vm.runInContext('machine = stub', context);
  return {
    send: (type, fields = {}) => context.self.onmessage({ data: { type, generation: 0, ...fields } }),
    frames,
    messages,
    override: fields => Object.assign(stub, fields),
    advance: ms => { now += ms; },
    setTick: tick => { guestTick = tick; },
    halt: () => { running = false; },
    trackUi: () => { uiTracking = true; },
    releasedAt: () => releasedAt,
  };
}

test('a brief key press is visible to one frame, then released', async () => {
  const w = worker();
  await w.send('keyDown', { macKey: 37, charCode: 108 });
  await w.send('keyUp', { macKey: 37, charCode: 108 });
  await w.send('frame');
  await w.send('frame');
  assert.deepEqual(w.frames.map(f => f.keys), [[37], []]);
});

test('held keys persist and release without an extra frame of delay', async () => {
  const w = worker();
  await w.send('keyDown', { macKey: 126 });
  await w.send('frame');
  await w.send('frame');
  await w.send('keyUp', { macKey: 126 });
  await w.send('frame');
  assert.deepEqual(w.frames.map(f => f.keys), [[126], [126], []]);
});

test('a presentation-only frame cannot consume the pending press', async () => {
  let steps = 0;
  const w = worker(() => steps);
  await w.send('keyDown', { macKey: 37 });
  await w.send('keyUp', { macKey: 37 });
  await w.send('frame');
  steps = 1;
  await w.send('frame');
  await w.send('frame');
  assert.deepEqual(w.frames.map(f => f.keys), [[37], [37], []]);
});

test('brief input lasts through several short CPU slices', async () => {
  const w = worker(() => 1, 2);
  await w.send('keyDown', { macKey: 37 });
  await w.send('keyUp', { macKey: 37 });
  for (let i = 0; i < 4; i++) await w.send('frame');
  assert.deepEqual(w.frames.map(f => f.keys), [[37], [37], [37], []]);
});

test('keyboard repeat does not extend the minimum release time', async () => {
  const w = worker();
  await w.send('keyDown', { macKey: 37 });
  await w.send('frame');
  await w.send('keyDown', { macKey: 37 });
  await w.send('keyUp', { macKey: 37 });
  await w.send('frame');
  assert.deepEqual(w.frames.map(f => f.keys), [[37], []]);
});

test('a duplicate release cannot expire the press using host time', async () => {
  const w = worker(() => 1, 2);
  await w.send('keyDown', { macKey: 37 });
  await w.send('keyUp', { macKey: 37 });
  await w.send('frame');
  w.advance(10_000);
  await w.send('keyUp', { macKey: 37 });
  await w.send('frame');
  await w.send('frame');
  await w.send('frame');
  assert.deepEqual(w.frames.map(f => f.keys), [[37], [37], [37], []]);
});

test('a brief click is visible to one frame and retains release coordinates', async () => {
  const w = worker();
  await w.send('mouseDown', { v: 10, h: 20 });
  await w.send('mouseUp', { v: 11, h: 21 });
  await w.send('frame');
  await w.send('frame');
  assert.deepEqual(w.frames.map(f => f.mouse), [true, false]);
  assert.deepEqual(w.releasedAt(), [11, 21]);
});

test('a new press is not cancelled by a previous pending release', async () => {
  const w = worker();
  await w.send('keyDown', { macKey: 37 });
  await w.send('keyUp', { macKey: 37 });
  await w.send('keyDown', { macKey: 37 });
  await w.send('frame');
  await w.send('frame');
  assert.deepEqual(w.frames.map(f => f.keys), [[37], [37]]);
});

test('host time does not release a key while guest ticks are stalled', async () => {
  const w = worker(() => 1, 0);
  await w.send('keyDown', { macKey: 37 });
  await w.send('keyUp', { macKey: 37 });
  w.advance(10_000);
  await w.send('frame');
  await w.send('frame');
  assert.deepEqual(w.frames.map(f => f.keys), [[37], [37]]);
  w.halt();
  await w.send('frame');
  await w.send('frame');
  assert.deepEqual(w.frames.at(-1).keys, []);
});

test('minimum press survives guest tick wraparound', async () => {
  const w = worker(() => 1, 2);
  w.setTick(0xfffffffa);
  await w.send('frame');
  await w.send('keyDown', { macKey: 37 });
  await w.send('keyUp', { macKey: 37 });
  for (let i = 0; i < 4; i++) await w.send('frame');
  assert.deepEqual(w.frames.slice(1).map(f => f.keys), [[37], [37], [37], []]);
});


test('a pending mouse release reaches menu tracking with frozen guest ticks', async () => {
  const w = worker(() => 1, 0);
  await w.send('mouseDown', { v: 10, h: 120 });
  await w.send('mouseUp', { v: 50, h: 120 });
  w.trackUi();
  await w.send('frame');
  await w.send('frame');
  assert.deepEqual(w.frames.map(f => f.mouse), [true, false]);
  assert.deepEqual(w.releasedAt(), [50, 120]);
});

test('release after entering tracking does not wait for another guest tick', async () => {
  const w = worker(() => 1, 0);
  await w.send('mouseDown', { v: 10, h: 120 });
  w.trackUi();
  await w.send('frame');
  await w.send('mouseUp', { v: 50, h: 120 });
  await w.send('frame');
  assert.deepEqual(w.frames.map(f => f.mouse), [true, false]);
});

test('tracking still gives new input a guest slice before releasing it', async () => {
  let steps = 0;
  const w = worker(() => steps, 0);
  w.trackUi();
  await w.send('frame');
  await w.send('keyDown', { macKey: 53 });
  await w.send('keyUp', { macKey: 53 });
  await w.send('frame');
  steps = 1;
  await w.send('frame');
  await w.send('frame');
  assert.deepEqual(w.frames.map(f => f.keys), [[], [53], [53], []]);
});


test('stale generation commands cannot affect the current runtime', async () => {
  const w = worker();
  await w.send('keyDown', { generation: 99, macKey: 37 });
  await w.send('frame', { generation: 99 });
  await w.send('frame');
  assert.deepEqual(w.frames.map(f => f.keys), [[]]);
  assert.equal(w.messages.length, 1);
  assert.equal(w.messages[0].generation, 0);
});

test('completed frames have monotonic sequence IDs even with frozen guest ticks', async () => {
  const w = worker(() => 1, 0);
  await w.send('frame');
  await w.send('frame');
  assert.deepEqual(w.messages.map(m => m.sequence), [1, 2]);
  assert.deepEqual(w.messages.map(m => m.guestTick), [0, 0]);
});

test('a failed runtime frame stops execution without retrying guest work', async () => {
  const w = worker(() => { throw new Error('guest failure'); });
  await w.send('frame');
  await w.send('frame');
  assert.equal(w.frames.length, 1);
  assert.equal(w.messages[0].type, 'error');
  assert.equal(w.messages[0].fatal, true);
  assert.match(w.messages[0].message, /guest failure/);
});

test('a save command failure remains visible without stopping gameplay', async () => {
  const w = worker();
  w.override({ importSave() { throw new Error('invalid save'); } });
  await w.send('importSave', { bytes: new Uint8Array() });
  await w.send('frame');
  assert.equal(w.messages[0].fatal, false);
  assert.equal(w.messages[0].operation, 'importSave');
  assert.equal(w.messages[1].type, 'frame');
});
