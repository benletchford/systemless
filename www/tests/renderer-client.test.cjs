const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const source = name => fs.readFileSync(path.join(__dirname, '../src', name), 'utf8');

function clientFixture({ unavailable = false, transferFails = false, direct = false } = {}) {
  let now = 0;
  const workers = [], observers = [], intervals = new Map(), listeners = new Map();
  let timerId = 0;
  const style = () => ({values:{},setProperty(name,value){this.values[name]=value;}});
  const parent = { style:style(), clientLeft: 1, clientTop: 1, scrollLeft: 0, scrollTop: 0, getBoundingClientRect() { return { left: 10, top: 20 }; }, children: [], insertBefore(node) { this.children.push(node); node.parentElement = this; } };
  const logical = { style:style(), width: 2, height: 1, offsetLeft: 12, offsetTop: 34, offsetWidth: 640, offsetHeight: 320,
    parentElement: parent, nextSibling: null, attributes: {},
    getBoundingClientRect() { return { left: 11 + this.offsetLeft, top: 21 + this.offsetTop, width: this.offsetWidth, height: this.offsetHeight }; },
    setAttribute(name, value) { this.attributes[name] = value; },
    transferControlToOffscreen() { throw new Error('input canvas must never transfer'); },
    focusToken: {}, listenersToken: {}, getContext() { throw new Error('input context must not be claimed'); } };
  if (unavailable) delete logical.transferControlToOffscreen;
  const document = { visibilityState: 'visible', createElement(type) {
    assert.equal(type, 'canvas');
    return { width: 0, height: 0, style: {}, attributes: {},
      setAttribute(name, value) { this.attributes[name] = value; },
      transferControlToOffscreen() { if (transferFails) throw new Error('cannot transfer'); this.transferred = true; return { offscreen: true }; },
      remove() { parent.children = parent.children.filter(node => node !== this); } };
  } };
  const owner = { messages: [], callback: null,
    postMessage(message) { this.messages.push(message); },
    addEventListener(name, callback) { this.callback = callback; },
    removeEventListener() { this.callback = null; } };
  const context = vm.createContext({ Uint8Array, Uint32Array, ArrayBuffer, document,
    MessageChannel: class { constructor() {
      this.port1 = {close(){this.closed=true;}}; this.port2 = {close(){this.closed=true;}};
    } },
    performance: { now: () => now },
    window: { addEventListener: (name, cb) => listeners.set(name, cb), removeEventListener: name => listeners.delete(name) },
    setInterval: callback => { intervals.set(++timerId, callback); return timerId; }, clearInterval: id => intervals.delete(id),
    ResizeObserver: class { constructor(callback) { this.callback = callback; this.targets = []; this.disconnected = false; observers.push(this); }
      observe(target) { this.targets.push(target); } disconnect() { this.disconnected = true; } },
    Worker: class { constructor(url) { this.url = url; this.messages = []; workers.push(this); }
      postMessage(message, transfer) { this.messages.push(message.type === 'frame' ? structuredClone(message, { transfer }) : message); }
      terminate() { this.terminated = true; } },
  });
  vm.runInContext(source('renderer-transport.js').replace('export class', 'class') + '\n'
    + source('renderer-client.js').replace(/^import .*;\n/m, '').replace('export class', 'class')
    + '\nthis.Client = RendererClient;', context);
  const client = new context.Client(logical, '/renderer-worker.js?runtime=test', 7, {direct,owner});
  const receive = fields => workers[0].onmessage?.({ data: { ...client.identity, ...fields } });
  return { client, logical, parent, workers, observers, intervals, listeners, document, receive, owner,
    ready: () => receive({ type: 'ready', kinds: ['rgba'] }),
    tick: ms => { now += ms; [...intervals.values()].forEach(callback => callback()); } };
}
const pixels = value => new Uint8Array([value, 2, 3, 255, 4, 5, 6, 255]);

test('only the fresh display canvas transfers; input ownership and geometry remain stable', () => {
  const f = clientFixture();
  const focus = f.logical.focusToken, listeners = f.logical.listenersToken;
  const canvas = f.parent.children[0];
  assert.equal(canvas.transferred, true); assert.equal(canvas.style.pointerEvents, 'none');
  assert.equal(canvas.style.left, '12px'); assert.equal(canvas.style.top, '34px');
  f.logical.offsetWidth = 400; f.observers[0].callback(); assert.equal(canvas.style.width, '400px');
  f.client.dispose();
  assert.equal(f.logical.focusToken, focus); assert.equal(f.logical.listenersToken, listeners);
  assert.equal(f.parent.children.length, 0); assert.equal(f.intervals.size, 0); assert.equal(f.listeners.size, 0);
  assert.equal(f.workers[0].terminated, true); assert.equal(f.observers[0].disconnected, true);
});

test('startup keeps only the newest complete frame and exposes it only after submission', () => {
  const f = clientFixture();
  f.client.paint(2, 1, pixels(1)); f.client.paint(2, 1, pixels(2));
  assert.equal(f.workers[0].messages.length, 1); assert.equal(f.client.status().submitted, false);
  f.ready();
  const message = f.workers[0].messages[1]; assert.equal(message.sequence, 2); assert.equal(message.pixels[0], 2);
  assert.equal(f.parent.children[0].style.visibility, 'hidden');
  f.receive({ type: 'submitted', sequence: 2, renderMs: 1, buffer: message.pixels.buffer });
  assert.equal(f.client.status().submitted, true); assert.equal(f.parent.children[0].style.visibility, 'visible');
});

test('failed or unavailable transfer leaves input canvas usable and removes partial resources', () => {
  for (const options of [{ unavailable: true }, { transferFails: true }]) {
    const f = clientFixture(options);
    assert.equal(f.client.status().phase, 'failed'); assert.equal(f.parent.children.length, 0);
    assert.equal(f.intervals.size, 0); assert.equal(f.listeners.size, 0);
  }
});

test('renderer failure retains pending pixels for fallback without touching the guest', () => {
  const f = clientFixture(); f.ready();
  f.client.paint(2, 1, pixels(1)); const latest = pixels(2); f.client.paint(2, 1, latest);
  f.workers[0].onerror({ preventDefault() {}, message: 'crashed' });
  assert.equal(f.client.status().phase, 'failed'); assert.equal(f.client.takeRecoveryFrame().pixels, latest);
  assert.equal(f.client.takeRecoveryFrame(), null); assert.equal(f.workers.length, 1);
  assert.equal(f.parent.children.length, 0); assert.equal(f.workers[0].terminated, true);
});

test('timeout excludes hidden time and terminates a stalled boot or submission', () => {
  for (const afterBoot of [false, true]) {
    const f = clientFixture();
    if (afterBoot) { f.ready(); f.client.paint(2, 1, pixels(1)); }
    f.tick(500); f.document.visibilityState = 'hidden'; f.tick(60000);
    assert.notEqual(f.client.status().phase, 'failed');
    f.document.visibilityState = 'visible'; f.tick(10001);
    assert.equal(f.client.status().phase, 'failed'); assert.equal(f.intervals.size, 0);
  }
});

test('stale callbacks cannot revive a disposed client or overwrite a replacement generation', () => {
  const f = clientFixture(); const callback = f.workers[0].onmessage;
  f.receive({ type: 'ready', kinds: ['rgba'], rendererGeneration: -1 });
  assert.equal(f.client.status().phase, 'booting');
  f.client.dispose(); callback({ data: { ...f.client.identity, type: 'ready', kinds: ['rgba'] } });
  assert.equal(f.client.status().phase, 'disposed'); assert.equal(f.parent.children.length, 0);
});


test('fractional canvas placement survives parent borders and scroll', () => {
  const f = clientFixture();
  f.logical.offsetLeft = 12.375; f.logical.offsetTop = 34.625;
  f.logical.offsetWidth = 639.5; f.logical.offsetHeight = 319.75;
  f.parent.scrollLeft = 2; f.parent.scrollTop = 3;
  f.observers[0].callback();
  const style = f.parent.children[0].style;
  assert.equal(style.left, '14.375px'); assert.equal(style.top, '37.625px');
  assert.equal(style.width, '639.5px'); assert.equal(style.height, '319.75px');
});


test('format changes advance display generation and retain only the newest complete packet', () => {
  const f = clientFixture();
  f.receive({ type: 'ready', backend: 'offscreen-webgl', kinds: ['rgba', 'indexed8'] });
  f.client.paint(2,1,pixels(1));
  const indexed = {kind:'indexed8',complete:true,width:2,height:1,stride:3,pixels:new Uint8Array(3),palette:new Uint8Array(1024)};
  f.client.paintPacket(indexed);
  assert.equal(f.client.transport.pending.displayGeneration,2);
  f.client.paint(2,1,pixels(2));
  assert.equal(f.client.transport.pending.kind,'rgba');
  assert.equal(f.client.transport.pending.displayGeneration,3);
});


test('direct handoff discards queued host images and keeps newer credit across acknowledgements', () => {
  const f = clientFixture({direct:true});
  f.client.paint(2,1,pixels(1)); f.ready();
  assert.equal(f.workers[0].messages.at(-1).type,'connectOwner');
  assert.equal(f.owner.messages[0].type,'connectRenderer');
  assert.equal(f.client.status().needsSnapshot,true);
  const count = f.workers[0].messages.length;
  f.client.paint(2,1,pixels(2));
  assert.equal(f.workers[0].messages.length,count);
  const status = fields => f.owner.callback({data:{...f.client.identity,type:'rendererStatus',...fields}});
  status({event:'queued',sequence:2});
  status({event:'queued',sequence:3});
  f.receive({type:'directSubmitted',width:4,height:2,outputScale:2,sequence:2,kind:'rgba',bytes:8,renderMs:1});
  status({event:'submitted',sequence:2,kind:'rgba',bytes:8,elapsedMs:4,renderMs:1});
  assert.equal(f.logical.width,4);assert.equal(f.logical.attributes['data-output-scale'],'2');
  assert.equal(f.parent.style.values['--game-aspect-ratio'],'4 / 2');
  assert.equal(f.client.directInFlight,3);
  assert.equal(f.client.status().needsSnapshot,false);
  assert.equal(f.client.status().pending,true);
  assert.equal(f.parent.children[0].style.visibility,'visible');
  f.receive({type:'directSubmitted',width:2,height:1,outputScale:1,sequence:3,kind:'rgba',bytes:8,renderMs:1});
  status({event:'queued',sequence:3}); // delayed owner notice must not reopen a completed UI wait
  status({event:'submitted',sequence:3,kind:'rgba',bytes:8,elapsedMs:4,renderMs:1});
  assert.equal(f.client.status().pending,false);
  assert.equal(f.logical.width,2);assert.equal(f.logical.attributes['data-output-scale'],'1');
  status({event:'error',message:'context lost'});
  assert.equal(f.client.status().phase,'failed');
  assert.equal(f.owner.messages.at(-1).type,'disconnectRenderer');
  assert.equal(f.owner.callback,null);
  assert.equal(f.parent.children.length,0);
});

test('direct handshake timeout releases the port route and requests ordinary fallback', () => {
  const f=clientFixture({direct:true});f.ready();f.tick(1);f.tick(10000);
  assert.equal(f.client.status().phase,'failed');
  assert.equal(f.owner.messages.at(-1).type,'disconnectRenderer');
});
