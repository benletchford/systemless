const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { Miniflare, convertV4MiniflareOptions } = require('miniflare');
const visitor = 'a'.repeat(32);
async function fixture(t, migrate = true) {
  const mf = new Miniflare(convertV4MiniflareOptions({
    modulesRoot: path.join(__dirname, '../build'),
    modules: [
      { type: 'ESModule', path: path.join(__dirname, '../build/index.js') },
      { type: 'CompiledWasm', path: path.join(__dirname, '../build/index_bg.wasm') },
    ],
    compatibilityDate: '2026-06-04',
    d1Databases: { POPULARITY: 'popularity' },
    serviceBindings: { ASSETS: () => Response.json(['doom']) },
  }));
  t.after(() => mf.dispose());
  const db = await mf.getD1Database('POPULARITY');
  if (migrate) {
    const sql = fs.readFileSync(path.join(__dirname, '../../migrations/0001_popularity.sql'), 'utf8');
    // D1 exec treats newlines as statement separators; keep the trigger together.
    await db.exec(sql.replace(/--[^\n]*/g, '').replace(/\s+/g, ' '));
  }
  const get = (url = 'https://systemless.org/api/popularity') => mf.dispatchFetch(url);
  const post = (body = { game: 'doom', visitor }, headers = {}, host = 'systemless.org') => mf.dispatchFetch(`https://${host}/api/play`, {
    method: 'POST', headers: { Origin: `https://${host}`, 'Content-Type': 'application/json', ...headers },
    body: typeof body === 'string' ? body : JSON.stringify(body),
  });
  return { mf, db, get, post };
}
test('compiled Worker atomically deduplicates plays and prunes expired rows', async t => {
  const { db, post } = await fixture(t);
  const responses = await Promise.all(Array.from({ length: 5 }, () => post()));
  assert.ok(responses.every(r => r.status === 204));
  assert.equal((await post({ game: 'doom', visitor: 'b'.repeat(32) })).status, 204);
  assert.equal(await db.prepare('SELECT plays FROM daily_plays').first('plays'), 2);
  await db.prepare("INSERT INTO play_visitors VALUES ('2000-01-01', ?, 'doom')").bind(visitor).run();
  await post();
  assert.equal(await db.prepare("SELECT COUNT(*) AS n FROM daily_plays WHERE day = '2000-01-01'").first('n'), 0);
  assert.equal(await db.prepare("SELECT COUNT(*) AS n FROM play_visitors WHERE day = '2000-01-01'").first('n'), 0);
});
test('validation, previews, method checks and daily cap preserve the database', async t => {
  const { db, post, get, mf } = await fixture(t);
  for (const [body, headers, host, status] of [
    [{}, {}, 'preview.pages.dev', 503],
    [undefined, { Origin: 'https://evil.example' }, undefined, 403],
    [undefined, { 'Content-Type': 'text/plain' }, undefined, 415],
    [{ game: 'missing', visitor }, {}, undefined, 400],
    [{ game: 'doom', visitor: 'bad' }, {}, undefined, 400],
    ['{', {}, undefined, 400],
    ['', {}, undefined, 400],
    ['x'.repeat(513), {}, undefined, 413],
  ]) assert.equal((await post(body, headers, host)).status, status);
  assert.equal((await mf.dispatchFetch('https://systemless.org/api/play')).status, 405);
  assert.equal((await mf.dispatchFetch('https://systemless.org/api/popularity', { method: 'POST' })).status, 405);
  assert.deepEqual(await (await get('https://preview.pages.dev/api/popularity')).json(), { plays: {} });
  const streamed = new ReadableStream({ start(controller) {
    controller.enqueue(new TextEncoder().encode('x'.repeat(300)));
    controller.enqueue(new TextEncoder().encode('x'.repeat(300)));
    controller.close();
  } });
  assert.equal((await mf.dispatchFetch('https://systemless.org/api/play', {
    method: 'POST', headers: { Origin: 'https://systemless.org', 'Content-Type': 'application/json' },
    body: streamed, duplex: 'half',
  })).status, 413);
  assert.equal(await db.prepare('SELECT COUNT(*) AS n FROM daily_plays').first('n'), 0);
  const day = new Date().toISOString().slice(0, 10);
  await db.batch(Array.from({ length: 50 }, (_, i) => db.prepare('INSERT INTO play_visitors VALUES (?, ?, ?)').bind(day, visitor, `game-${i}`)));
  assert.equal((await post()).status, 204);
  assert.equal(await db.prepare("SELECT COUNT(*) AS n FROM daily_plays WHERE game = 'doom'").first('n'), 0);
  assert.equal((await mf.dispatchFetch('https://systemless.org/')).status, 200);
});
test('rankings include 30 UTC dates and share one cache entry across queries', async t => {
  const { db, get } = await fixture(t);
  for (const [age, count] of [[0, 3], [29, 2], [30, 100]]) {
    const day = new Date(Date.now() - age * 86400000).toISOString().slice(0, 10);
    await db.prepare("INSERT INTO daily_plays VALUES (?, 'doom', ?)").bind(day, count).run();
  }
  const response = await get('https://systemless.org/api/popularity?one');
  assert.deepEqual(await response.json(), { plays: { doom: 5 } });
  assert.equal(response.headers.get('Cache-Control'), 'public, max-age=300, s-maxage=3600');
  await db.prepare('DELETE FROM daily_plays').run();
  assert.deepEqual(await (await get('https://systemless.org/api/popularity?two')).json(), { plays: { doom: 5 } });
});
test('database failures return no-store fallbacks', async t => {
  const { get, post } = await fixture(t, false);
  const response = await get();
  assert.equal(response.status, 503);
  assert.equal(response.headers.get('Cache-Control'), 'no-store');
  assert.deepEqual(await response.json(), { plays: {} });
  assert.equal((await post()).status, 503);
});
