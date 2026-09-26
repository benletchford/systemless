const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function fixture(search = '?renderer=worker') {
  const source = fs.readFileSync(path.join(__dirname, '../src/renderer_bridge.rs'), 'utf8');
  const js = source.match(/inline_js = r#"([\s\S]*?)"#\)\]/)[1].replace(/export /g, '')
    .replace('import(moduleUrl.href)', 'loadRendererModule(moduleUrl.href)');
  const timers = new Map(), imports = [], clients = [];
  let id = 0, now = 0, resolve, reject;
  const module = new Promise((yes, no) => { resolve = yes; reject = no; });
  const document = { visibilityState: 'visible' };
  const context = vm.createContext({ URL, URLSearchParams, document,
    location: { search, href: `https://systemless.test/game${search}` },
    ResizeObserver: class {}, Worker: class {},
    performance: { now: () => now, getEntriesByType: () => [{ name: 'https://systemless.test/systemless-a_bg.wasm' }] },
    setInterval: callback => { timers.set(++id, callback); return id; }, clearInterval: id => timers.delete(id),
    loadRendererModule: url => { imports.push(url); return module; },
  });
  vm.runInContext(js, context);
  const handle = context.createSystemlessRenderer({ transferControlToOffscreen() {} }, 7);
  class RendererClient {
    constructor(...args) { this.args = args; this.paints = []; clients.push(this); }
    paint(...args) { this.paints.push(args); }
    status() { return { phase: 'ready', submitted: false, pending: false }; }
    takeRecoveryFrame() { return null; }
    dispose() { this.disposed = true; }
  }
  return { context, handle, clients, imports, timers, document,
    resolve: () => resolve({ RendererClient }), reject,
    tick: ms => { now += ms; [...timers.values()].forEach(callback => callback()); },
    settle: async () => { for (let i = 0; i < 6; i++) await Promise.resolve(); } };
}

test('renderer modules are loaded only with the explicit test option', () => {
  const f = fixture(''); assert.equal(f.handle, null); assert.equal(f.imports.length, 0); assert.equal(f.timers.size, 0);
});

test('module startup retains one newest frame and uses the runtime asset identity', async () => {
  const f = fixture(); const a = new Uint8Array(8), b = new Uint8Array(8);
  f.context.paintSystemlessRenderer(f.handle, 2, 1, a);
  f.context.paintSystemlessRenderer(f.handle, 2, 1, b);
  f.resolve(); await f.settle();
  assert.equal(f.clients.length, 1); assert.equal(f.clients[0].paints.length, 1);
  assert.equal(f.clients[0].paints[0][2], b); assert.equal(f.clients[0].args[2], 7);
  assert.match(f.imports[0], /runtime=https%3A%2F%2Fsystemless.test%2Fsystemless-a_bg.wasm/);
  assert.equal(f.timers.size, 0);
});

test('navigation during module loading cannot create an orphan presenter', async () => {
  const f = fixture(); f.context.disposeSystemlessRenderer(f.handle);
  f.resolve(); await f.settle(); assert.equal(f.clients.length, 0); assert.equal(f.timers.size, 0);
});

test('module rejection is observed and returns retained pixels for fallback', async () => {
  const f = fixture(); const pixels = new Uint8Array(8);
  f.context.paintSystemlessRenderer(f.handle, 2, 1, pixels);
  f.reject(new Error('missing module')); await f.settle();
  assert.equal(f.context.systemlessRendererStatus(f.handle).phase, 'failed');
  assert.equal(f.context.takeSystemlessRendererRecovery(f.handle).pixels, pixels);
  assert.equal(f.context.takeSystemlessRendererRecovery(f.handle), null); assert.equal(f.timers.size, 0);
});

test('module timeout ignores hidden time and rejects a late successful import', async () => {
  const f = fixture(); f.document.visibilityState = 'hidden'; f.tick(60000);
  assert.equal(f.context.systemlessRendererStatus(f.handle).phase, 'booting');
  f.document.visibilityState = 'visible'; f.tick(10001);
  assert.equal(f.context.systemlessRendererStatus(f.handle).phase, 'failed');
  f.resolve(); await f.settle(); assert.equal(f.clients.length, 0);
});
