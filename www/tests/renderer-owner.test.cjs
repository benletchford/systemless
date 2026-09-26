const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const read = name => fs.readFileSync(path.join(__dirname, '../src', name), 'utf8');
const identity = { generation: 8, rendererGeneration: 4 };
function fixture() {
  const messages = [], notices = [];
  const port = { start() {}, close() { this.closed = true; },
    postMessage(message, transfer) { messages.push(structuredClone(message, { transfer })); } };
  const context = vm.createContext({ Uint8Array, Uint32Array, ArrayBuffer, performance: { now: () => 15 } });
  vm.runInContext(read('renderer-transport.js').replace('export class', 'class') + '\n'
    + read('renderer-owner.js').replace(/^import .*;\n/m, '').replaceAll('export ', '')
    + '\nthis.Owner = RendererOwner;', context);
  const owner = new context.Owner(port, identity, { sequence: 12, displayGeneration: 3, notify: message => notices.push(message) });
  const ack = message => port.onmessage({ data: { ...identity, protocolVersion: 4, type: 'submitted', sequence: message.sequence,
    buffer: message.pixels?.buffer ?? message.compact?.cells.buffer, detailBuffer: message.compact?.detail.buffer,
    paletteBuffer: message.palette?.buffer, renderMs: 2 } });
  return { owner, port, messages, notices, ack };
}
const rgba = value => ({ width: 1, height: 1, frame: new Uint8Array([value, 2, 3, 255]), audio: new Uint8Array([8]), guestTick: 100 });

test('direct owner retains only active and newest complete packets across frozen ticks and formats', () => {
  const f = fixture(), first = rgba(1), audio = first.audio;
  const pixels = first.frame;
  f.owner.submit(first);
  assert.equal(pixels.byteLength, 0);
  assert.equal(first.frame, undefined);
  assert.equal(first.audio, audio);
  assert.equal(first.guestTick, 100);
  for (let i = 0; i < 500; i++) f.owner.submit(rgba(i));
  const last = { width: 2, height: 2, outputScale: 2,
    compactFrame: {kind:'compact',complete:true,width:2,height:2,
      compact:{width:1,height:1,scale:2,cells:new Uint32Array([0x123456]),detail:new Uint32Array(0)}} };
  f.owner.submit(last);
  assert.equal(f.messages.length, 1);
  assert.equal(f.owner.transport.recycled.length, 2);
  assert.equal(last.compactFrame, undefined);
  f.ack(f.messages[0]);
  assert.equal(f.messages.length, 2);
  assert.equal(f.messages[1].kind, 'compact');
  assert.equal(f.messages[1].sequence, 514);
  assert.equal(f.messages[1].displayGeneration, 5);
  // New send notification precedes previous completion; the host must retain
  // the newer active sequence when observing the previous submission.
  assert.deepEqual(f.notices.map(n => [n.event,n.sequence]), [['queued',13],['queued',514],['submitted',13]]);
  f.ack(f.messages[1]);
  assert.equal(f.owner.transport.inFlight, null);
  assert.equal(f.owner.transport.pending, null);
  f.owner.dispose();
  assert.equal(f.port.closed, true);
  assert.equal(f.owner.transport.recycled.length, 0);
});

test('direct renderer failure reports presenter status and closes port without touching guest output', () => {
  const f = fixture();
  f.owner.submit(rgba(1));
  f.port.onmessage({ data: {...identity,protocolVersion:4,type:'error',message:'context lost'} });
  assert.equal(f.owner.closed,true);
  assert.equal(f.port.closed,true);
  assert.equal(f.notices.at(-1).type,'rendererStatus');
  assert.equal(f.notices.at(-1).event,'error');
  const next = rgba(2);
  assert.equal(f.owner.submit(next),false);
  assert.equal(next.frame.byteLength,4);
});
