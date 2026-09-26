const { test } = require("node:test");
const assert = require("node:assert/strict");

test("missing phase samples do not claim zero cost", async () => {
  const { percentiles } = await import("../scripts/runtime-metrics.mjs");
  assert.deepEqual(percentiles([undefined, null, NaN, Infinity]), {
    count: 0, p50: null, p95: null, p99: null,
  });
});

test("nearest-rank percentiles retain ordinary costs and tail stalls", async () => {
  const { percentiles } = await import("../scripts/runtime-metrics.mjs");
  const values = Array(98).fill(10).concat([100, 200]).reverse();
  assert.deepEqual(percentiles(values), { count: 100, p50: 10, p95: 10, p99: 100 });
  assert.equal(values[0], 200);
  assert.deepEqual(percentiles([16.625]), { count: 1, p50: 16.6, p95: 16.6, p99: 16.6 });
});

async function probeProgress(worker, target, maximumMs) {
  const fs = require('node:fs');
  const path = require('node:path');
  const vm = require('node:vm');
  const source = fs.readFileSync(path.join(__dirname, '../scripts/verify-runtime-pacing-cdp.mjs'), 'utf8');
  const functionSource = source.slice(source.indexOf('async function runtimeProbe('), source.indexOf('\nfunction runtimeTracePrelude('));
  let now = 0;
  const frames = [];
  const canvas = {
    width: 640, height: 480,
    getAttribute(name) {
      if (name === 'data-runtime-worker') return String(worker);
      if (name === 'data-runtime-game-id') return 'fixture';
      return null;
    },
  };
  const context = vm.createContext({
    performance: { now: () => now },
    navigator: { userAgent: 'test' }, devicePixelRatio: 1,
    document: { visibilityState: 'visible', querySelector: selector => selector === 'canvas.game-canvas' ? canvas : null },
    window: {
      addEventListener() {},
      __systemlessWorkerTrace: worker ? frames : [],
      __systemlessFrameTrace: worker ? [] : frames,
    },
    requestAnimationFrame(callback) {
      queueMicrotask(() => {
        now += 16;
        frames.push({ guestTick: 600 + now / 8, totalInstructions: now * 100 });
        callback(now);
      });
    },
  });
  vm.runInContext(functionSource, context);
  return context.runtimeProbe(maximumMs, false, target);
}

for (const worker of [false, true]) {
  test(`guest-progress endpoint records frame overshoot (${worker ? 'worker' : 'compatibility'})`, async () => {
    const report = await probeProgress(worker, 603, 1000);
    assert.equal(report.progress_endpoint.reached, true);
    assert.equal(report.progress_endpoint.observed_tick, 604);
    assert.equal(report.progress_endpoint.observed_instructions, 3200);
    assert.equal(report.samples.length, 2);
  });
}

test('unreached guest progress times out without claiming success', async () => {
  const report = await probeProgress(true, 1000, 32);
  assert.equal(report.progress_endpoint.reached, false);
  assert.equal(report.progress_endpoint.observed_tick, 604);
  assert.equal(report.samples.length, 2);
});
