const { test } = require("node:test");
const assert = require("node:assert/strict");
const { RULE, configure } = require("../scripts/configure-popularity-rate-limit.cjs");
const env = { CLOUDFLARE_ACCOUNT_ID: "a".repeat(32), CLOUDFLARE_API_TOKEN: "test-secret" };
const zone = { id: "b".repeat(32), name: "systemless.org", account: { id: env.CLOUDFLARE_ACCOUNT_ID } };
const desired = { ...RULE, id: "our-rule" };
function mock(existing) {
  const calls = [];
  let ruleset = existing;
  return { calls, fetch: async (url, options) => {
    calls.push({ url, ...options });
    let result;
    if (url.includes("/zones?")) result = [zone];
    else if (options.method === "POST" || options.method === "PATCH") {
      ruleset = { id: "ruleset", rules: [...(ruleset?.rules || []).filter(rule => rule.ref !== RULE.ref), desired] };
      result = ruleset;
    } else if (!ruleset) return Response.json({ success: false }, { status: 404 });
    else result = ruleset;
    return Response.json({ success: true, result });
  } };
}

test("creates the rate-limit entrypoint when absent and verifies it", async () => {
  const api = mock(null);
  assert.equal((await configure(env, api.fetch)).changed, true);
  const writes = api.calls.filter(call => call.method);
  assert.equal(writes.length, 1);
  assert.equal(writes[0].method, "POST");
  assert.deepEqual(JSON.parse(writes[0].body).rules, [RULE]);
});

test("adds only our rule without replacing another rule", async () => {
  const other = { id: "other", ref: "unrelated" };
  const api = mock({ id: "ruleset", rules: [other] });
  await configure(env, api.fetch);
  const writes = api.calls.filter(call => call.method);
  assert.equal(writes.length, 1);
  assert.match(writes[0].url, /\/rulesets\/ruleset\/rules$/);
  assert.deepEqual(JSON.parse(writes[0].body), RULE);
});

test("an active matching rule is not rewritten", async () => {
  const api = mock({ id: "ruleset", rules: [desired] });
  assert.equal((await configure(env, api.fetch)).changed, false);
  assert.equal(api.calls.filter(call => call.method).length, 0);
});

test("updates only our existing rule when its threshold has drifted", async () => {
  const api = mock({ id: "ruleset", rules: [{ ...desired, ratelimit: { ...RULE.ratelimit, requests_per_period: 100 } }] });
  await configure(env, api.fetch);
  const writes = api.calls.filter(call => call.method);
  assert.equal(writes.length, 1);
  assert.equal(writes[0].method, "PATCH");
  assert.match(writes[0].url, /\/rules\/our-rule$/);
});

test("permission errors stop setup without writing or exposing secrets", async () => {
  let calls = 0;
  await assert.rejects(configure(env, async (url, options) => {
    calls++;
    assert.equal(options.method, undefined);
    return Response.json({ success: false, errors: [{ code: 10000, message: env.CLOUDFLARE_API_TOKEN }] }, { status: 403 });
  }), error => error.message.includes("Zone Read") && !error.message.includes(env.CLOUDFLARE_API_TOKEN));
  assert.equal(calls, 1);
});
