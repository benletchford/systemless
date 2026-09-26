const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function serviceWorker({ online = true, cached = 'stale' } = {}) {
  const handlers = new Map();
  const requests = [];
  const writes = [];
  const cache = {
    match: async () => cached === null ? undefined : new Response(cached),
    put: async (request, response) => writes.push([request.url, await response.text()]),
  };
  const context = vm.createContext({
    URL, Response,
    self: {
      location: { origin: 'https://systemless.test' },
      addEventListener: (name, callback) => handlers.set(name, callback),
    },
    caches: { open: async () => cache, match: cache.match },
    fetch: async request => {
      requests.push(request.url);
      if (!online) throw new Error('offline');
      return new Response('current');
    },
  });
  vm.runInContext(fs.readFileSync(path.join(__dirname, '../public/service-worker.js'), 'utf8'), context);
  return {
    requests, writes,
    async fetch(asset) {
      let response;
      handlers.get('fetch')({
        request: { url: `https://systemless.test${asset}`, method: 'GET', mode: 'cors' },
        respondWith(value) { response = value; },
      });
      return (await response).text();
    },
  };
}

for (const asset of ['/emulator-worker.js', '/renderer-worker.js', '/renderer-transport.js', '/renderer-client.js', '/snippets/systemless-org-hash/inline0.js']) {
  test(`${asset} checks the network before using stale cached code`, async () => {
    const sw = serviceWorker();
    assert.equal(await sw.fetch(asset), 'current');
    assert.equal(sw.requests.length, 1);
    assert.deepEqual(sw.writes, [[`https://systemless.test${asset}`, 'current']]);
  });
  test(`${asset} can return offline cached code for runtime protocol validation`, async () => {
    const sw = serviceWorker({ online: false });
    assert.equal(await sw.fetch(asset), 'stale');
    assert.equal(sw.requests.length, 1);
  });
}

test('hashed Wasm assets retain their cache-first path', async () => {
  const sw = serviceWorker({ cached: 'immutable wasm' });
  assert.equal(await sw.fetch('/systemless-org-123_bg.wasm'), 'immutable wasm');
  assert.equal(sw.requests.length, 0);
});
