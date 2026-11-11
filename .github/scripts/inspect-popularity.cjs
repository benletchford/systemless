// Fixed SELECT statements only. Keep identifiers and credentials out of logs.
const fs = require("node:fs");
const queries = [
  ["totals", "SELECT datetime('now') AS checked_at_utc, COUNT(*) AS daily_rows, COALESCE(SUM(plays), 0) AS recorded_plays, COUNT(DISTINCT game) AS games, MIN(day) AS first_day, MAX(day) AS latest_day FROM daily_plays"],
  ["recent_games", "SELECT day, game, plays FROM daily_plays ORDER BY day DESC, plays DESC, game LIMIT 50"],
  ["deduplication", "SELECT COUNT(*) AS records, COUNT(DISTINCT visitor) AS anonymous_daily_identifiers, MIN(day) AS first_day, MAX(day) AS latest_day FROM play_visitors"],
  ["today_mismatches", "SELECT p.day, p.game, p.plays, COALESCE(v.entries, 0) AS dedupe_entries FROM daily_plays p LEFT JOIN (SELECT day, game, COUNT(*) AS entries FROM play_visitors WHERE day = date('now') GROUP BY day, game) v ON p.day = v.day AND p.game = v.game WHERE p.day = date('now') AND p.plays != COALESCE(v.entries, 0) LIMIT 50"],
];

async function main() {
  const { CLOUDFLARE_ACCOUNT_ID: account, CLOUDFLARE_API_TOKEN: token,
    CLOUDFLARE_POPULARITY_DATABASE_ID: database } = process.env;
  if (!/^[a-f0-9]{32}$/i.test(account || "") || !token
      || !/^[a-f0-9-]{36}$/i.test(database || "")) throw new Error("Missing Cloudflare inspection configuration.");
  const response = await fetch(`https://api.cloudflare.com/client/v4/accounts/${account}/d1/database/${database}/query`, {
    method: "POST",
    headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
    body: JSON.stringify({ sql: queries.map(([, sql]) => sql).join("; ") }),
    signal: AbortSignal.timeout(30000),
  });
  const body = await response.json();
  if (!response.ok || !body.success || body.result?.length !== queries.length
      || body.result.some(result => !result.success)) {
    throw new Error(`D1 inspection failed (HTTP ${response.status}).`);
  }
  const report = Object.fromEntries(queries.map(([name], i) => [name, body.result[i].results]));
  const output = JSON.stringify(report, null, 2);
  console.log(output);
  if (process.env.GITHUB_STEP_SUMMARY) fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, `\n\`\`\`json\n${output}\n\`\`\`\n`);
}
main().catch(error => { console.error(error.message); process.exitCode = 1; });
