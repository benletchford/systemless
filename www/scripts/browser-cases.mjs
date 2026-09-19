import { readFileSync } from "node:fs";

// Probe inputs are supplied by the tester; this frontend owns no game fixtures.
export function browserCases() {
  const path = process.env.SYSTEMLESS_BROWSER_CASES;
  if (!path) throw new Error("Set SYSTEMLESS_BROWSER_CASES to a JSON array of browser probe cases");
  const cases = JSON.parse(readFileSync(path, "utf8"));
  if (!Array.isArray(cases) || !cases.length || cases.some(c => !c.id || !c.route)) {
    throw new Error("Browser probe cases must be a nonempty array with id and route fields");
  }
  return cases;
}
