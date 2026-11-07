const DAY = 86400000;
const reply = (status) => new Response(null, { status, headers: { "Cache-Control": "no-store" } });

export async function onRequest({ request, env }) {
  const url = new URL(request.url);
  if (request.method !== "POST") return reply(405);
  if (url.hostname !== "systemless.org" || !env.POPULARITY) return reply(503);
  if (request.headers.get("Origin") !== url.origin) return reply(403);
  if (request.headers.get("Content-Type") !== "application/json") return reply(415);
  // Bound the body even when Content-Length is missing or dishonest.
  const reader = request.body?.getReader();
  if (!reader) return reply(400);
  let body = "";
  const decoder = new TextDecoder();
  let length = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    length += value.byteLength;
    if (length > 512) { await reader.cancel(); return reply(413); }
    body += decoder.decode(value, { stream: true });
  }
  body += decoder.decode();
  let game, visitor;
  try { ({ game, visitor } = JSON.parse(body)); } catch { return reply(400); }
  if (typeof game !== "string" || !/^[a-z0-9][a-z0-9-]{0,99}$/.test(game)
      || typeof visitor !== "string" || !/^[a-f0-9]{32}$/.test(visitor)) return reply(400);
  try {
    const catalogue = await env.ASSETS.fetch(new Request(`${url.origin}/popularity-games.json`));
    if (!catalogue.ok) return reply(503);
    if (!(await catalogue.json()).includes(game)) return reply(400);
    const day = new Date().toISOString().slice(0, 10);
    const oldest = new Date(Date.now() - 29 * DAY).toISOString().slice(0, 10);
    await env.POPULARITY.batch([
      // Bounded, indexed expiry avoids a scheduler and large cleanup spikes.
      env.POPULARITY.prepare(`DELETE FROM play_visitors WHERE (day, visitor, game) IN
        (SELECT day, visitor, game FROM play_visitors WHERE day < ? LIMIT 100)`).bind(day),
      env.POPULARITY.prepare(`DELETE FROM daily_plays WHERE (day, game) IN
        (SELECT day, game FROM daily_plays WHERE day < ? LIMIT 100)`).bind(oldest),
      // The primary key and trigger make deduplication/counting atomic, including concurrent tabs.
      // Cap each anonymous browser at 50 different games per day.
      env.POPULARITY.prepare(`INSERT OR IGNORE INTO play_visitors (day, visitor, game)
        SELECT ?, ?, ? WHERE
        (SELECT COUNT(*) FROM play_visitors WHERE day = ? AND visitor = ?) < 50`)
        .bind(day, visitor, game, day, visitor),
    ]);
    return reply(204);
  } catch {
    return reply(503);
  }
}
