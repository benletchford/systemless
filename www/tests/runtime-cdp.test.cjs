const assert = require("node:assert/strict");
const { test } = require("node:test");

class FakeSocket extends EventTarget {
  static last;
  constructor() { super(); FakeSocket.last = this; queueMicrotask(() => this.dispatchEvent(new Event("open"))); }
  send(value) { this.sent = JSON.parse(value); }
  close() { this.dispatchEvent(new Event("close")); }
  reply(result) {
    this.dispatchEvent(new MessageEvent("message", { data: JSON.stringify({ id: this.sent.id, result }) }));
  }
}

test("CDP replies resolve and disconnection rejects pending and future commands", async () => {
  const { connect } = await import("../scripts/runtime-cdp.mjs");
  const client = connect("test", { WebSocketImpl: FakeSocket });
  await client.ready;
  const first = client.send("Page.enable");
  FakeSocket.last.reply({ enabled: true });
  assert.deepEqual(await first, { enabled: true });
  const pending = client.send("Runtime.evaluate");
  FakeSocket.last.close();
  await assert.rejects(pending, /connection closed/);
  await assert.rejects(client.send("Page.enable"), /connection closed/);
});

test("CDP commands time out rather than hanging when no response arrives", async () => {
  const { connect } = await import("../scripts/runtime-cdp.mjs");
  const client = connect("test", { WebSocketImpl: FakeSocket, timeoutMs: 10 });
  await client.ready;
  await assert.rejects(client.send("Page.enable"), /command timed out: Page.enable/);
  client.close();
});

test("asynchronous event failure rejects the active probe", async () => {
  const { connect } = await import("../scripts/runtime-cdp.mjs");
  const client = connect("test", { WebSocketImpl: FakeSocket });
  await client.ready;
  client.on("Fetch.requestPaused", async () => { throw new Error("archive unavailable"); });
  const pending = client.send("Runtime.evaluate");
  FakeSocket.last.dispatchEvent(new MessageEvent("message", { data: JSON.stringify({ method: "Fetch.requestPaused" }) }));
  await assert.rejects(pending, /archive unavailable/);
});

function fakePage({ truncate = false } = {}) {
  let remote;
  let released = false;
  let largestMessage = 0;
  return {
    get released() { return released; },
    get largestMessage() { return largestMessage; },
    async send(method, params) {
      if (method === "Runtime.evaluate") {
        assert.equal(params.returnByValue, false);
        remote = await eval(params.expression);
        return { result: { objectId: "probe" } };
      }
      assert.equal(params.objectId, "probe");
      if (method === "Runtime.releaseObject") { released = true; remote = null; return {}; }
      assert.equal(method, "Runtime.callFunctionOn");
      const fn = eval(`(${params.functionDeclaration})`);
      let value = fn.apply(remote, params.arguments.map(arg => arg.value));
      if (truncate && typeof value === "string") value = value.slice(1);
      const response = { result: { value } };
      const size = JSON.stringify(response).length;
      largestMessage = Math.max(largestMessage, size);
      assert.ok(size < 4 * 1024 * 1024, "oversized CDP message");
      return response;
    },
  };
}

test("large reports survive bounded chunks without losing Unicode or escaped content", async () => {
  const { evaluateJson } = await import("../scripts/runtime-cdp.mjs");
  const page = fakePage();
  const expression = `Promise.resolve({ trace: "a\\\\\\\"😀".repeat(900000), endpoint: 4801 })`;
  const expected = await eval(expression);
  assert.ok(JSON.stringify(expected).length > 4 * 1024 * 1024);
  assert.deepEqual(await evaluateJson(page, expression, 1000), expected);
  assert.ok(page.largestMessage < 2 * 1024 * 1024);
  assert.equal(page.released, true);
});

test("incomplete reports fail and release the remote result", async () => {
  const { evaluateJson } = await import("../scripts/runtime-cdp.mjs");
  const page = fakePage({ truncate: true });
  await assert.rejects(evaluateJson(page, "({ sample: 42 })", 1000), /Incomplete diagnostic JSON chunk/);
  assert.equal(page.released, true);
});
