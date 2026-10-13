const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function fixture(dpr = 2) {
  const listeners = [];
  const window = {
    devicePixelRatio: dpr, screenX: 20, screenY: 40,
    screen: { width: 1792, height: 1120 },
    innerWidth: 1000, innerHeight: 700, outerWidth: 1000, outerHeight: 800,
    visualViewport: { scale: 1, offsetLeft: 0, offsetTop: 0 },
    addEventListener: (type, callback, options) => listeners.push({ type, callback, options }),
  };
  const source = fs.readFileSync(path.join(__dirname, '../src/browser_bridge.rs'), 'utf8');
  const js = source.match(/inline_js = r#"([\s\S]*?)"#\)\]/)[1].replace(/export /g, '');
  const context = vm.createContext({ window });
  vm.runInContext(js, context);
  const scale = () => context.systemlessCursorZoom();
  const move = (x, y, zoom, extra = {}) => {
    const event = { clientX: x, clientY: y, screenX: 22 + x * zoom,
      screenY: 109 + y * zoom, pointerType: 'mouse', isTrusted: true, ...extra };
    for (const listener of listeners) listener.callback(event);
  };
  return { window, context, listeners, scale, move };
}

test('screen/client coordinates distinguish page zoom from Retina scaling', () => {
  for (const density of [1, 2]) {
    for (const zoom of [0.8, 1, 1.25, 2, 4]) {
      const f = fixture(density * zoom);
      f.scale();
      f.move(10, 100, zoom);
      f.move(10.5, 100, zoom);
      f.move(20, 100, zoom);
      assert.ok(Math.abs(f.scale() - zoom) < 1e-9);
      // The 800-DIP game fits its canvas into fewer CSS pixels when zoomed.
      assert.equal(Math.round(16 * (800 / zoom) / 800 * f.scale()), 16);
    }
  }
});

test('keyboard zoom updates the cursor without waiting for mouse movement', () => {
  const f = fixture();
  assert.equal(f.scale(), 1);
  f.window.devicePixelRatio = 2.5;
  f.window.innerWidth = 800;
  assert.equal(f.scale(), 1.25);
  f.window.devicePixelRatio = 2;
  f.window.innerWidth = 1000;
  assert.equal(f.scale(), 1);
  assert.equal(f.listeners.length, 1);
  assert.equal(f.listeners[0].options.passive, true);
});

test('docked tools and side panels change content width without changing cursor scale', () => {
  const f = fixture();
  f.scale();
  f.move(10, 10, 1);
  f.window.innerWidth = 500;
  assert.equal(f.scale(), 1);
  f.move(100, 20, 1);
  f.move(200, 20, 1);
  assert.equal(f.scale(), 1);
});

test('moving to another display preserves zoom and resets the coordinate baseline', () => {
  const f = fixture(2.5);
  f.scale(); f.move(10, 10, 1.25); f.move(50, 30, 1.25);
  f.window.devicePixelRatio = 1.25;
  f.window.screenX = 1800;
  f.window.screen.width = 1920;
  assert.equal(f.scale(), 1.25);
  f.move(200, 200, 1.25, { screenX: 2200, screenY: 500 });
  assert.equal(f.scale(), 1.25);
  f.move(240, 240, 1.25, { screenX: 2250, screenY: 550 });
  assert.equal(f.scale(), 1.25);
});

test('synthetic, touch and pen events cannot recalibrate the mouse cursor', () => {
  const f = fixture(); f.scale(); f.move(0, 0, 1);
  for (const extra of [{ isTrusted: false }, { pointerType: 'touch' }, { pointerType: 'pen' }, { clientX: NaN }]) {
    f.move(100, 100, 4, extra);
    assert.equal(f.scale(), 1);
  }
  f.move(100, 100, 1);
  assert.equal(f.scale(), 1);
});

test('layout and pinch changes discard deltas measured in the previous coordinate space', () => {
  const f = fixture(); f.scale(); f.move(0, 0, 1);
  f.window.visualViewport.scale = 2;
  assert.equal(f.scale(), 2);
  f.move(100, 50, 2); f.move(200, 100, 2);
  assert.equal(f.scale(), 2);
  f.window.visualViewport.offsetLeft = 40;
  f.move(100, 50, 2, { screenX: 900, screenY: 700 });
  assert.equal(f.scale(), 2);
});

test('a fresh coordinate measurement corrects an in-place display density change', () => {
  const f = fixture(); f.scale(); f.move(10, 10, 1); f.move(50, 50, 1);
  f.window.devicePixelRatio = 3;
  f.scale();
  f.move(20, 20, 1); f.move(100, 100, 1);
  assert.equal(f.scale(), 1);
});

test('worker module imports do not require a Window', () => {
  const f = fixture(); delete f.context.window;
  assert.equal(f.scale(), 1);
  assert.equal(f.listeners.length, 0);
});
