const fs = require("node:fs");
const RULE = {
  ref: "systemless_popularity_play",
  description: "Limit Systemless play submissions per IP",
  expression: '(http.request.uri.path eq "/api/play")',
  action: "block",
  enabled: true,
  ratelimit: {
    characteristics: ["cf.colo.id", "ip.src"],
    period: 10,
    requests_per_period: 5,
    mitigation_timeout: 10,
  },
};

function matches(rule) {
  return rule?.ref === RULE.ref && rule.expression === RULE.expression
    && rule.action === RULE.action && rule.enabled === true
    && rule.ratelimit?.period === 10 && rule.ratelimit?.requests_per_period === 5
    && rule.ratelimit?.mitigation_timeout === 10
    && JSON.stringify([...(rule.ratelimit?.characteristics || [])].sort())
      === JSON.stringify([...RULE.ratelimit.characteristics].sort())
    && !rule.ratelimit?.counting_expression && !rule.ratelimit?.requests_to_origin;
}

async function configure(env, fetchImpl = fetch) {
  if (!env.CLOUDFLARE_API_TOKEN || !/^[a-f0-9]{32}$/i.test(env.CLOUDFLARE_ACCOUNT_ID || "")) {
    throw new Error("Missing Cloudflare account ID or API token.");
  }
  async function api(endpoint, options = {}, allowMissing = false) {
    const response = await fetchImpl(`https://api.cloudflare.com/client/v4${endpoint}`, {
      ...options,
      headers: { Authorization: `Bearer ${env.CLOUDFLARE_API_TOKEN}`, "Content-Type": "application/json" },
      signal: AbortSignal.timeout(30000),
    });
    if (response.status === 404 && allowMissing) return null;
    const body = await response.json();
    if (!response.ok || body.success !== true) {
      const codes = (body.errors || []).map(error => Number(error.code)).filter(Number.isFinite).join(", ");
      throw new Error(`Cloudflare request failed (HTTP ${response.status}, codes ${codes}). The token needs Zone Read and Zone WAF Edit/Write for systemless.org. Existing rate-limit rules and plan capacity may also prevent creation.`);
    }
    return body.result;
  }
  const zones = await api(`/zones?name=systemless.org&account.id=${env.CLOUDFLARE_ACCOUNT_ID}`);
  const zone = zones.find(item => item.name === "systemless.org" && item.account?.id === env.CLOUDFLARE_ACCOUNT_ID);
  if (!zone || !/^[a-f0-9]{32}$/i.test(zone.id)) {
    throw new Error("systemless.org is not visible in this account. Grant Zone Read for systemless.org to the existing Cloudflare token.");
  }
  const base = `/zones/${zone.id}/rulesets`;
  const entrypoint = `${base}/phases/http_ratelimit/entrypoint`;
  const existing = await api(entrypoint, {}, true);
  const owned = (existing?.rules || []).filter(rule => rule.ref === RULE.ref);
  if (owned.length > 1) throw new Error("Multiple managed popularity rules found; refusing to change the ruleset.");
  const current = owned[0];
  let changed = false;
  if (!matches(current)) {
    if (!existing) {
      await api(base, { method: "POST", body: JSON.stringify({ name: "Systemless rate limits", kind: "zone", phase: "http_ratelimit", rules: [RULE] }) });
    } else if (current) {
      await api(`${base}/${existing.id}/rules/${current.id}`, { method: "PATCH", body: JSON.stringify(RULE) });
    } else {
      // Append only our rule; never replace or delete another rule to make room.
      await api(`${base}/${existing.id}/rules`, { method: "POST", body: JSON.stringify(RULE) });
    }
    changed = true;
  }
  const verified = await api(entrypoint);
  const rule = verified.rules?.find(rule => rule.ref === RULE.ref);
  if (!matches(rule)) throw new Error("The configured rule did not match the requested rate limit on readback.");
  return { changed, zone: zone.id, ruleset: verified.id, rule: rule.id };
}

async function main() {
  const result = await configure(process.env);
  const summary = `Popularity rate limit ${result.changed ? "configured" : "already active"}: 5 requests / 10 seconds / IP, 10-second block on /api/play. Rule ${result.rule}.`;
  console.log(summary);
  if (process.env.GITHUB_STEP_SUMMARY) fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${summary}\n`);
}
module.exports = { RULE, matches, configure };
if (require.main === module) main().catch(error => { console.error(error.message); process.exitCode = 1; });
