const DAY = 86400000;
const production = (url) => url.hostname === "systemless.org";
const json = (body, status = 200, headers = {}) => Response.json(body, {
  status, headers: { "Cache-Control": "no-store", ...headers },
});

export async function onRequest(context) {
  const { request, env } = context;
  const url = new URL(request.url);
  if (request.method !== "GET") return json({ error: "Method not allowed" }, 405);
  if (!production(url) || !env.POPULARITY) return json({ plays: {} });
  // Ignore query strings so callers cannot multiply cache entries.
  const key = new Request(`${url.origin}/api/popularity`);
  const cache = caches.default;
  const cached = await cache.match(key);
  if (cached) return cached;
  try {
    const since = new Date(Date.now() - 29 * DAY).toISOString().slice(0, 10);
    const { results } = await env.POPULARITY.prepare(
      "SELECT game, SUM(plays) AS plays FROM daily_plays WHERE day >= ? GROUP BY game",
    ).bind(since).all();
    const response = json({ plays: Object.fromEntries(results.map(row => [row.game, row.plays])) },
      200, { "Cache-Control": "public, max-age=300, s-maxage=3600" });
    context.waitUntil(cache.put(key, response.clone()));
    return response;
  } catch {
    return json({ plays: {} }, 503);
  }
}
