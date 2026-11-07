const { test } = require("node:test");
const assert = require("node:assert/strict");
const { databaseId } = require("../scripts/configure-popularity.cjs");
const uuid = "11111111-2222-3333-4444-555555555555";
const env = { CLOUDFLARE_ACCOUNT_ID: "a".repeat(32), CLOUDFLARE_API_TOKEN: "secret-test-token" };

test("setup reuses the existing exact-name database without creating resources", async () => {
  const calls = [];
  const result = await databaseId(env, async (url, options) => {
    calls.push({ url, options });
    return Response.json({ success: true, result: [{ name: "systemless-popularity", uuid }] });
  });
  assert.equal(result, uuid);
  assert.equal(calls.length, 1);
  assert.match(calls[0].url, /name=systemless-popularity/);
  assert.equal(calls[0].options.method, undefined);
});

test("setup creates only the missing database and returns its UUID", async () => {
  const calls = [];
  assert.equal(await databaseId(env, async (url, options) => {
    calls.push(options);
    return Response.json({ success: true, result: calls.length === 1 ? [] : { uuid } });
  }), uuid);
  assert.equal(calls.length, 2);
  assert.equal(calls[1].method, "POST");
  assert.deepEqual(JSON.parse(calls[1].body), { name: "systemless-popularity" });
});

test("denied lookup fails without attempting creation or exposing credentials", async () => {
  let calls = 0;
  await assert.rejects(databaseId(env, async () => {
    calls++;
    return Response.json({ success: false, errors: [env.CLOUDFLARE_API_TOKEN] }, { status: 403 });
  }), error => error.message.includes("403") && !error.message.includes(env.CLOUDFLARE_API_TOKEN));
  assert.equal(calls, 1);
});

test("an explicit valid UUID needs no Cloudflare API call", async () => {
  assert.equal(await databaseId({ CLOUDFLARE_POPULARITY_DATABASE_ID: uuid }, () => { throw Error("unexpected API call"); }), uuid);
  await assert.rejects(databaseId({ CLOUDFLARE_POPULARITY_DATABASE_ID: "bad" }), /Invalid/);
});
