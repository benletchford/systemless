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
