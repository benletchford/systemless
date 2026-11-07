const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const vm = require("node:vm");
const { execFileSync } = require("node:child_process");
const root = path.join(__dirname, "..");

function load(relative, globals, exports) {
  const source = fs.readFileSync(path.join(root, relative), "utf8").replaceAll("export function", "function").replaceAll("export async function", "async function");
  return vm.runInNewContext(`${source}\n;({${exports.join(",")}})`, { Response, Request, URL, Date, TextDecoder, ...globals });
}

function database(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "systemless-popularity-"));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const db = path.join(dir, "test.sqlite");
  const run = (sql, values = [], schema = false) => JSON.parse(execFileSync("python3", ["-c", `
import sqlite3,json,sys
con=sqlite3.connect(sys.argv[1])
con.row_factory=sqlite3.Row
q=json.load(sys.stdin)
if q['schema']:
 con.executescript(q['sql']); result=[]
else:
 result=[dict(r) for r in con.execute(q['sql'],q['values'])]
con.commit()
print(json.dumps(result))
`, db], { input: JSON.stringify({ sql, values, schema }), encoding: "utf8" }));
  run(fs.readFileSync(path.join(root, "migrations/0001_popularity.sql"), "utf8"), [], true);
  return {
    run,
    prepare(sql) {
      return { bind(...values) { return { sql, values, async all() { return { results: run(sql, values) }; } }; } };
    },
    async batch(statements) { for (const s of statements) run(s.sql, s.values); },
  };
}
const visitor = "a".repeat(32);
function request(body = { game: "doom", visitor }, headers = {}, host = "systemless.org") {
  return new Request(`https://${host}/api/play`, { method: "POST", headers: {
    Origin: `https://${host}`, "Content-Type": "application/json", ...headers,
  }, body: JSON.stringify(body) });
}

test("play counting deduplicates in SQLite, aggregates browsers and expires old rows", async t => {
  const db = database(t);
  const { onRequest } = load("functions/api/play.js", {}, ["onRequest"]);
  const env = { POPULARITY: db, ASSETS: { fetch: async () => Response.json(["doom"]) } };
  for (const token of [visitor, visitor, "b".repeat(32)]) {
    assert.equal((await onRequest({ request: request({ game: "doom", visitor: token }), env })).status, 204);
  }
  assert.equal(db.run("SELECT plays FROM daily_plays")[0].plays, 2);
  db.run("INSERT INTO play_visitors VALUES ('2000-01-01', ?, 'doom')", [visitor]);
  await onRequest({ request: request(), env });
  assert.equal(db.run("SELECT * FROM play_visitors WHERE day = '2000-01-01'").length, 0);
  assert.equal(db.run("SELECT * FROM daily_plays WHERE day = '2000-01-01'").length, 0);
});

test("invalid requests and previews do not write; browser contribution is bounded", async t => {
  const db = database(t);
  const { onRequest } = load("functions/api/play.js", {}, ["onRequest"]);
  const env = { POPULARITY: db, ASSETS: { fetch: async () => Response.json(["doom"]) } };
  for (const [req, status] of [
    [request({}, {}, "preview.pages.dev"), 503],
    [request(undefined, { Origin: "https://evil.example" }), 403],
    [request({ game: "missing", visitor }), 400],
    [request({ game: "doom", visitor: "bad" }), 400],
    [request({ padding: "x".repeat(600) }), 413],
  ]) assert.equal((await onRequest({ request: req, env })).status, status);
  assert.equal(db.run("SELECT * FROM daily_plays").length, 0);
  const day = new Date().toISOString().slice(0, 10);
  for (let i = 0; i < 50; i++) db.run("INSERT INTO play_visitors VALUES (?, ?, ?)", [day, visitor, `game-${i}`]);
  await onRequest({ request: request(), env });
  assert.equal(db.run("SELECT * FROM daily_plays WHERE game = 'doom'").length, 0);
});

test("rankings use 30 UTC dates, cache independently of query string, and fail gracefully", async t => {
  const db = database(t);
  const today = new Date().toISOString().slice(0, 10);
  const oldest = new Date(Date.now() - 29 * 86400000).toISOString().slice(0, 10);
  const expired = new Date(Date.now() - 30 * 86400000).toISOString().slice(0, 10);
  for (const [day, count] of [[today, 3], [oldest, 2], [expired, 100]]) db.run("INSERT INTO daily_plays VALUES (?, 'doom', ?)", [day, count]);
  const entries = new Map();
  const pending = [];
  const cache = { async match(key) { return entries.get(key.url)?.clone(); }, async put(key, value) { entries.set(key.url, value); } };
  const { onRequest } = load("functions/api/popularity.js", { caches: { default: cache } }, ["onRequest"]);
  const context = { request: new Request("https://systemless.org/api/popularity?bust=1"), env: { POPULARITY: db }, waitUntil: promise => pending.push(promise) };
  const response = await onRequest(context);
  assert.deepEqual(await response.json(), { plays: { doom: 5 } });
  await Promise.all(pending);
  assert.equal(entries.size, 1);
  context.env.POPULARITY = { prepare() { throw new Error("offline"); } };
  context.request = new Request("https://systemless.org/api/popularity?bust=2");
  assert.equal((await onRequest(context)).status, 200);
  entries.clear();
  const failed = await onRequest(context);
  assert.equal(failed.status, 503);
  assert.deepEqual(await failed.json(), { plays: {} });
});

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
