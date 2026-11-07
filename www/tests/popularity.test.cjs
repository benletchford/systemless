const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const root = path.join(__dirname, "..");

function load(relative, globals, exports) {
  const source = fs.readFileSync(path.join(root, relative), "utf8").replaceAll("export function", "function").replaceAll("export async function", "async function");
  return vm.runInNewContext(`${source}\n;({${exports.join(",")}})`, { Response, Request, URL, Date, TextDecoder, ...globals });
}

function browser() {
  let now = 0, callback, posts = [];
  const listeners = new Map();
  const storage = new Map();
  const document = { visibilityState: "visible", addEventListener(type, fn) { listeners.set(type, fn); }, removeEventListener(type) { listeners.delete(type); } };
  const canvas = { isConnected: true };
  const api = load("src/popularity.js", {
    location: { hostname: "systemless.org", pathname: "/games/doom/" }, navigator: {}, document,
    performance: { now: () => now }, crypto: require("node:crypto").webcrypto,
    localStorage: { getItem: k => storage.get(k) || null, setItem: (k, v) => storage.set(k, v) },
    setInterval: fn => { callback = fn; return 1; }, clearInterval: () => { callback = null; },
    fetch: async (url, options) => { posts.push(JSON.parse(options.body)); return { ok: true }; },
  }, ["startPlay", "playHeartbeat"]);
  return { api, canvas, posts, listeners, document, tick(running = true) { now += 1000; if (running) api.playHeartbeat(canvas, true); callback?.(); } };
}

test("only visible running time with trusted game input qualifies, once per day", async () => {
  const b = browser();
  b.api.startPlay(b.canvas, "doom");
  for (let i = 0; i < 35; i++) b.tick();
  assert.equal(b.posts.length, 0);
  b.listeners.get("keydown")({ target: b.canvas, isTrusted: false });
  b.tick();
  assert.equal(b.posts.length, 0);
  b.listeners.get("keydown")({ target: b.canvas, isTrusted: true });
  b.tick();
  assert.equal(b.posts.length, 1);
  assert.equal(b.listeners.size, 0);
  await Promise.resolve();
  b.api.startPlay(b.canvas, "doom");
  b.listeners.get("keydown")({ target: b.canvas, isTrusted: true });
  for (let i = 0; i < 31; i++) b.tick();
  assert.equal(b.posts.length, 1);
});

test("hidden, stopped and unmounted games do not accrue plays", () => {
  const b = browser();
  b.api.startPlay(b.canvas, "doom");
  b.listeners.get("pointerdown")({ target: b.canvas, isTrusted: true });
  b.document.visibilityState = "hidden";
  b.listeners.get("visibilitychange")();
  for (let i = 0; i < 40; i++) b.tick();
  b.document.visibilityState = "visible";
  b.listeners.get("visibilitychange")();
  b.api.playHeartbeat(b.canvas, false);
  for (let i = 0; i < 40; i++) b.tick(false);
  assert.equal(b.posts.length, 0);
  for (let i = 0; i < 29; i++) b.tick();
  assert.equal(b.posts.length, 0);
  b.canvas.isConnected = false;
  b.tick();
  assert.equal(b.posts.length, 0);
  assert.equal(b.listeners.size, 0);
});
