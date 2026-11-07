// Reuse or provision the database, then configure this deployment's Pages binding.
const fs = require("node:fs");
const path = require("node:path");
const DATABASE_NAME = "systemless-popularity";
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

async function databaseId(env, fetchImpl = fetch) {
  if (env.CLOUDFLARE_POPULARITY_DATABASE_ID) {
    if (!UUID.test(env.CLOUDFLARE_POPULARITY_DATABASE_ID)) throw new Error("Invalid popularity database UUID.");
    return env.CLOUDFLARE_POPULARITY_DATABASE_ID;
  }
  if (!/^[a-f0-9]{32}$/i.test(env.CLOUDFLARE_ACCOUNT_ID || "") || !env.CLOUDFLARE_API_TOKEN) {
    throw new Error("Provide CLOUDFLARE_ACCOUNT_ID and CLOUDFLARE_API_TOKEN, or an existing CLOUDFLARE_POPULARITY_DATABASE_ID.");
  }
  const endpoint = `https://api.cloudflare.com/client/v4/accounts/${env.CLOUDFLARE_ACCOUNT_ID}/d1/database`;
  async function api(suffix, options = {}) {
    const response = await fetchImpl(endpoint + suffix, {
      ...options,
      headers: { Authorization: `Bearer ${env.CLOUDFLARE_API_TOKEN}`, "Content-Type": "application/json" },
      signal: AbortSignal.timeout(30000),
    });
    const body = await response.json();
    if (!response.ok || body.success !== true) {
      // Never log response bodies or request headers containing credentials.
      throw new Error(`Cloudflare D1 request failed (HTTP ${response.status}); check D1 Read/Write permissions.`);
    }
    return body.result;
  }
  const databases = await api(`?name=${DATABASE_NAME}&per_page=10000`);
  const matches = databases.filter(db => db.name === DATABASE_NAME);
  if (matches.length > 1) throw new Error("Multiple popularity databases found; specify the database UUID explicitly.");
  const database = matches[0] || await api("", { method: "POST", body: JSON.stringify({ name: DATABASE_NAME }) });
  if (!UUID.test(database.uuid || "")) throw new Error("Cloudflare returned an invalid database UUID.");
  return database.uuid;
}

async function main() {
  const id = await databaseId(process.env);
  const config = path.join(__dirname, "..", "wrangler.toml");
  const base = fs.readFileSync(config, "utf8").split("\n# Generated popularity binding")[0];
  fs.writeFileSync(config, `${base.trimEnd()}\n\n# Generated popularity binding
[[d1_databases]]
binding = "POPULARITY"
database_name = "${DATABASE_NAME}"
database_id = "${id}"
migrations_dir = "migrations"
`);
  console.log(`Configured ${DATABASE_NAME}: ${id}`);
  if (process.env.GITHUB_STEP_SUMMARY) {
    fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, `Popularity database: \`${DATABASE_NAME}\` (\`${id}\`).\n`);
  }
}

module.exports = { databaseId };
if (require.main === module) main().catch(error => { console.error(error.message); process.exitCode = 1; });
