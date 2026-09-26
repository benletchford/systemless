#!/usr/bin/env node

import { createReadStream, existsSync, statSync } from "node:fs";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { browserCases } from "./browser-cases.mjs";

import { spawn } from "node:child_process";

const DEFAULT_BASE_URL = "http://127.0.0.1:8080";
const DEFAULT_CHROME_PATHS = [
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/Applications/Chromium.app/Contents/MacOS/Chromium",
  "/usr/bin/google-chrome",
  "/usr/bin/google-chrome-stable",
  "/usr/bin/chromium",
  "/usr/bin/chromium-browser",
];

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(scriptDir, "..");
const baseUrl = (process.env.SYSTEMLESS_ORG_URL ?? DEFAULT_BASE_URL).replace(/\/$/, "");
const chromePath = process.env.CHROME_BIN ?? DEFAULT_CHROME_PATHS.find(existsSync);
const gameTimeoutMs = envNumber("SYSTEMLESS_SAVE_SMOKE_TIMEOUT_MS", 180_000);

if (!chromePath) {
  throw new Error("Set CHROME_BIN to a Chrome/Chromium executable for CDP verification");
}

const games = browserCases();

const selectedIds = new Set(
  (process.env.SYSTEMLESS_SAVE_SMOKE_GAMES ?? games.map((game) => game.id).join(","))
    .split(",")
    .map((id) => id.trim())
    .filter(Boolean),
);
const selected = games.filter((game) => selectedIds.has(game.id));
for (const id of selectedIds) {
  if (!games.some((game) => game.id === id)) {
    throw new Error(`Unknown save smoke game: ${id}`);
  }
}

const reports = [];
for (const game of selected) {
  if (!existsSync(game.archivePath)) {
    throw new Error(`Missing local archive for ${game.id}: ${game.archivePath}`);
  }

  process.stderr.write(`[save-files-smoke] ${game.id} ${game.route}\n`);
  reports.push(await runGameSmoke(game));
}

console.log(JSON.stringify({ games: reports }, null, 2));

async function runGameSmoke(game) {
  const archiveServer = await serveArchive(game.archivePath);
  const userDataDir = await mkdtemp(join(tmpdir(), "systemless-save-cdp-"));
  const port = 9437 + Math.floor(Math.random() * 1000);
  const chrome = spawn(
    chromePath,
    [
      "--headless=new",
      `--remote-debugging-port=${port}`,
      `--user-data-dir=${userDataDir}`,
      "--disable-gpu",
      "--autoplay-policy=no-user-gesture-required",
      "--no-first-run",
      "--no-default-browser-check",
      "--window-size=1280,900",
      "about:blank",
    ],
    { stdio: "ignore" },
  );

  let page;
  try {
    const version = await waitForChrome(port);
    const browser = connect(version.webSocketDebuggerUrl);
    await browser.ready;
    const { targetId } = await browser.send("Target.createTarget", { url: "about:blank" });
    browser.close();

    const targets = await fetchJson(`http://127.0.0.1:${port}/json/list`);
    const target = targets.find((candidate) => candidate.id === targetId);
    if (!target) {
      throw new Error("Chrome target was not listed after creation");
    }

    page = connect(target.webSocketDebuggerUrl);
    await page.ready;
    page.on("Fetch.requestPaused", (params) =>
      handleArchiveRequest(page, params, archiveServer, game),
    );
    await page.send("Page.enable");
    await page.send("Runtime.enable");
    await page.send("Page.addScriptToEvaluateOnNewDocument", {
      source: downloadCapturePrelude(),
    });
    await page.send("Fetch.enable", {
      patterns: [{ urlPattern: game.archiveUrl, requestStage: "Request" }],
    });
    await page.send("Page.navigate", { url: `${baseUrl}${game.route}` });

    const report = await evaluate(
      page,
      `(${saveSmokeProbe.toString()})(${JSON.stringify({
        gameId: game.id,
        pilotName: game.pilotName,
        actions: game.actions,
        timeoutMs: gameTimeoutMs,
      })})`,
      gameTimeoutMs + 30_000,
    );
    report.route = game.route;
    report.archive_server_requests = archiveServer.requests();
    assertSaveSmokeReport(report, game);
    return report;
  } finally {
    if (page && process.env.SYSTEMLESS_SAVE_SMOKE_SCREENSHOT_DIR) {
      try {
        const screenshot = await page.send("Page.captureScreenshot", { format: "png" });
        await writeFile(
          join(process.env.SYSTEMLESS_SAVE_SMOKE_SCREENSHOT_DIR, `${game.id}.png`),
          Buffer.from(screenshot.data, "base64"),
        );
      } catch (error) {
        process.stderr.write(`Could not capture save probe: ${error.message}\n`);
      }
    }
    page?.close();
    await archiveServer.close();
    chrome.kill("SIGTERM");
    await sleep(250);
    await rm(userDataDir, { recursive: true, force: true, maxRetries: 10, retryDelay: 250 });
  }
}

async function serveArchive(path) {
  const stat = statSync(path);
  let requests = 0;
  const server = createServer((req, res) => {
    if (req.url !== "/game.kpk") {
      res.writeHead(404, { "content-type": "text/plain" });
      res.end("not found");
      return;
    }
    requests += 1;
    res.writeHead(200, {
      "access-control-allow-origin": "*",
      "cache-control": "no-store",
      "content-type": "application/octet-stream",
      "content-length": String(stat.size),
    });
    createReadStream(path).pipe(res);
  });

  const url = await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      server.off("error", reject);
      const address = server.address();
      if (!address || typeof address === "string") {
        reject(new Error("archive server did not return a TCP address"));
        return;
      }
      resolve(`http://127.0.0.1:${address.port}/game.kpk`);
    });
  });

  return {
    url,
    requests() {
      return requests;
    },
    close() {
      return new Promise((resolve) => server.close(resolve));
    },
  };
}

async function handleArchiveRequest(page, params, archiveServer, game) {
  const url = params.request?.url ?? "";
  if (url !== game.archiveUrl) {
    await page.send("Fetch.failRequest", {
      requestId: params.requestId,
      errorReason: "BlockedByClient",
    });
    return;
  }

  await page.send("Fetch.continueRequest", {
    requestId: params.requestId,
    url: archiveServer.url,
  });
}

function downloadCapturePrelude() {
  return `(() => {
    const downloads = [];
    const objectUrls = new Map();
    const originalCreateObjectURL = URL.createObjectURL.bind(URL);
    const originalRevokeObjectURL = URL.revokeObjectURL.bind(URL);
    const originalClick = HTMLAnchorElement.prototype.click;
    window.__systemlessDownloadAttempts = downloads;
    URL.createObjectURL = (object) => {
      const url = originalCreateObjectURL(object);
      if (object instanceof Blob) {
        objectUrls.set(url, { size: object.size, type: object.type });
      }
      return url;
    };
    URL.revokeObjectURL = (url) => {
      originalRevokeObjectURL(url);
      objectUrls.delete(url);
    };
    HTMLAnchorElement.prototype.click = function() {
      const meta = objectUrls.get(this.href);
      if (this.download && meta) {
        downloads.push({ filename: this.download, size: meta.size, type: meta.type });
        return;
      }
      return originalClick.apply(this, arguments);
    };
  })();`;
}

async function saveSmokeProbe(config) {
  const errors = [];
  const startedAt = performance.now();
  window.addEventListener("error", (event) => {
    errors.push(String(event.message || event.error || "error"));
  });
  window.addEventListener("unhandledrejection", (event) => {
    errors.push(String(event.reason || "unhandled rejection"));
  });

  await waitForRuntime(config.gameId, config.timeoutMs);
  await runActions(config.actions, config.timeoutMs);
  const saved = await waitForSave(config.gameId, config.pilotName, config.timeoutMs);
  await clickSaveAction("download", saved.path);
  const download = await waitForDownload(config.pilotName, 5_000);
  await clickSaveAction("remove", saved.path);
  const removed = await waitForRemoval(config.gameId, saved.path, 10_000);

  return {
    game_id: config.gameId,
    pilot_name: config.pilotName,
    elapsed_ms: Math.round(performance.now() - startedAt),
    save_path: saved.path,
    save_data_fork_bytes: saved.dataForkBytes,
    save_resource_fork_bytes: saved.resourceForkBytes,
    shelf_text_before_remove: saved.shelfText,
    idb_records_before_remove: saved.records.length,
    idb_paths_before_remove: saved.records.map((record) => record.path),
    download,
    shelf_text_after_remove: removed.shelfText,
    idb_records_after_remove: removed.records.length,
    idb_paths_after_remove: removed.records.map((record) => record.path),
    console: errors,
  };

  async function waitForRuntime(gameId, timeoutMs) {
    await waitUntil(() => {
      const canvas = document.querySelector("canvas.game-canvas");
      if (!canvas) {
        return false;
      }
      canvas.focus();
      return canvas.getAttribute("data-runtime-game-id") === gameId;
    }, timeoutMs, `runtime ${gameId} did not become ready`);
  }

  async function runActions(actions, timeoutMs) {
    const deadline = performance.now() + timeoutMs;
    for (const action of actions) {
      if (performance.now() > deadline) {
        throw new Error(`timed out before action ${JSON.stringify(action)}`);
      }
      if (action.type === "run") {
        await sleep(action.ticks * 17);
      } else if (action.type === "run_until_pixel") {
        await runUntilPixel(action);
      } else if (action.type === "mouse_move") {
        pointerEvent("pointermove", action.v, action.h, { button: 0, buttons: 0 });
      } else if (action.type === "mouse_down") {
        pointerEvent("pointerdown", action.v, action.h, { button: 0, buttons: 1 });
      } else if (action.type === "mouse_up") {
        pointerEvent("pointerup", action.v, action.h, { button: 0, buttons: 0 });
      } else if (action.type === "key_down") {
        keyEvent("keydown", action.key);
      } else if (action.type === "key_up") {
        keyEvent("keyup", action.key);
      }
    }
  }

  async function runUntilPixel(action) {
    const timeoutMs = Math.max(1_000, action.timeout_ticks * 17);
    await waitUntil(() => {
      const [r, g, b] = canvasPixel(action.x, action.y);
      const tolerance = action.tolerance ?? 0;
      const matches =
        Math.abs(r - action.rgb[0]) <= tolerance &&
        Math.abs(g - action.rgb[1]) <= tolerance &&
        Math.abs(b - action.rgb[2]) <= tolerance;
      return action.not ? !matches : matches;
    }, timeoutMs, `pixel condition failed: ${action.label ?? JSON.stringify(action)}`);
  }

  async function waitForSave(gameId, pilotName, timeoutMs) {
    let latest = null;
    await waitUntil(async () => {
      const records = await readSaveRecords(gameId);
      const shelfText = saveShelfText();
      const record = records.find((candidate) => fileName(candidate.path) === pilotName);
      if (!record || !shelfText.includes(pilotName)) {
        latest = { records, shelfText };
        return false;
      }
      latest = { records, shelfText, record };
      return true;
    }, timeoutMs, `save ${pilotName} was not persisted and shown in the shelf`).catch((error) => {
      throw new Error(`${error.message}; observed paths=${JSON.stringify(latest?.records.map(record => record.path))}; shelf=${JSON.stringify(latest?.shelfText)}`);
    });
    const record = latest.record;
    return {
      path: record.path,
      dataForkBytes: base64Bytes(record.data_fork_b64),
      resourceForkBytes: base64Bytes(record.resource_fork_b64),
      shelfText: latest.shelfText,
      records: latest.records,
    };
  }

  async function clickSaveAction(action, path) {
    const selector = `[data-save-action="${action}"][data-save-path="${cssEscape(path)}"]`;
    const button = document.querySelector(selector);
    if (!button) {
      throw new Error(`missing save ${action} button for ${path}`);
    }
    button.click();
  }

  async function waitForDownload(pilotName, timeoutMs) {
    let latest = null;
    await waitUntil(() => {
      const attempts = window.__systemlessDownloadAttempts || [];
      latest = attempts.at(-1) || null;
      return latest && latest.filename === `${pilotName}.bin` && latest.size > 0;
    }, timeoutMs, `download was not attempted for ${pilotName}`);
    return latest;
  }

  async function waitForRemoval(gameId, path, timeoutMs) {
    let latest = null;
    await waitUntil(async () => {
      const records = await readSaveRecords(gameId);
      const shelfText = saveShelfText();
      latest = { records, shelfText };
      return !records.some((record) => record.path === path) && !shelfText.includes(fileName(path));
    }, timeoutMs, `save ${path} was not removed from shelf and IndexedDB`);
    return latest;
  }

  function canvas() {
    const element = document.querySelector("canvas.game-canvas");
    if (!element) {
      throw new Error("game canvas missing");
    }
    element.focus();
    return element;
  }

  function pointerEvent(type, v, h, init) {
    const element = canvas();
    const rect = element.getBoundingClientRect();
    const clientX = rect.left + (h / element.width) * rect.width;
    const clientY = rect.top + (v / element.height) * rect.height;
    const eventInit = {
      bubbles: true,
      cancelable: true,
      pointerId: 1,
      pointerType: "mouse",
      isPrimary: true,
      clientX,
      clientY,
      ...init,
    };
    if (window.PointerEvent) {
      element.dispatchEvent(new PointerEvent(type, eventInit));
    } else {
      const mouseType =
        type === "pointerdown" ? "mousedown" : type === "pointerup" ? "mouseup" : "mousemove";
      element.dispatchEvent(new MouseEvent(mouseType, eventInit));
    }
  }

  function keyEvent(type, scriptKey) {
    const { key, code } = browserKey(scriptKey);
    canvas().dispatchEvent(
      new KeyboardEvent(type, {
        key,
        code,
        bubbles: true,
        cancelable: true,
      }),
    );
  }

  function browserKey(scriptKey) {
    if (scriptKey === "return") {
      return { key: "Enter", code: "Enter" };
    }
    if (scriptKey === "space") {
      return { key: " ", code: "Space" };
    }
    if (scriptKey === "tab") {
      return { key: "Tab", code: "Tab" };
    }
    if (scriptKey === "backspace") {
      return { key: "Backspace", code: "Backspace" };
    }
    if (scriptKey.length === 1 && /[A-Za-z]/.test(scriptKey)) {
      return { key: scriptKey, code: `Key${scriptKey.toUpperCase()}` };
    }
    if (scriptKey.length === 1 && /[0-9]/.test(scriptKey)) {
      return { key: scriptKey, code: `Digit${scriptKey}` };
    }
    return { key: scriptKey, code: scriptKey };
  }

  function canvasPixel(x, y) {
    const element = canvas();
    const ctx = element.getContext("2d", { willReadFrequently: true });
    const clampedX = Math.max(0, Math.min(element.width - 1, x));
    const clampedY = Math.max(0, Math.min(element.height - 1, y));
    return Array.from(ctx.getImageData(clampedX, clampedY, 1, 1).data.slice(0, 3));
  }

  function saveShelfText() {
    return document.querySelector(".save-panel")?.textContent ?? "";
  }

  async function readSaveRecords(gameId) {
    const db = await openSaveDb();
    try {
      const tx = db.transaction("files", "readonly");
      const store = tx.objectStore("files");
      const values = await requestToPromise(store.getAll());
      return values
        .map((value) => {
          try {
            return typeof value === "string" ? JSON.parse(value) : null;
          } catch (_) {
            return null;
          }
        })
        .filter((record) => record && record.game_id === gameId)
        .sort((left, right) => left.path.localeCompare(right.path));
    } finally {
      db.close();
    }
  }

  function openSaveDb() {
    return new Promise((resolve, reject) => {
      const request = indexedDB.open("systemless-save-files", 1);
      request.onupgradeneeded = () => {
        const db = request.result;
        if (!db.objectStoreNames.contains("files")) {
          db.createObjectStore("files");
        }
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(new Error("IndexedDB open failed"));
    });
  }

  function requestToPromise(request) {
    return new Promise((resolve, reject) => {
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(new Error("IndexedDB request failed"));
    });
  }

  async function waitUntil(predicate, timeoutMs, message) {
    const deadline = performance.now() + timeoutMs;
    let lastError;
    while (performance.now() < deadline) {
      try {
        if (await predicate()) {
          return;
        }
      } catch (error) {
        lastError = error;
      }
      await sleep(100);
    }
    throw new Error(lastError ? `${message}: ${lastError.message}` : message);
  }

  function base64Bytes(value) {
    if (!value) {
      return 0;
    }
    return atob(value).length;
  }

  function fileName(path) {
    return String(path).split("/").filter(Boolean).at(-1) ?? String(path);
  }

  function cssEscape(value) {
    if (window.CSS && CSS.escape) {
      return CSS.escape(value);
    }
    return String(value).replace(/["\\]/g, "\\$&");
  }

  function sleep(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }
}

function assertSaveSmokeReport(report, game) {
  const failures = [];
  if (report.game_id !== game.id) {
    failures.push(`game id ${report.game_id} did not match ${game.id}`);
  }
  if (!report.save_path || !report.save_path.endsWith(`/Pilots/${game.pilotName}`)) {
    failures.push(`save path ${JSON.stringify(report.save_path)} did not end with /Pilots/${game.pilotName}`);
  }
  if (report.save_data_fork_bytes <= 0 && report.save_resource_fork_bytes <= 0) {
    failures.push("persisted save had no fork bytes");
  }
  if (report.download?.filename !== `${game.pilotName}.bin`) {
    failures.push(`download filename was ${JSON.stringify(report.download?.filename)}`);
  }
  if (report.download?.size <= 0) {
    failures.push("download blob was empty");
  }
  if (report.idb_paths_after_remove.includes(report.save_path)) {
    failures.push(`IndexedDB still had removed save ${report.save_path}`);
  }
  if (report.shelf_text_after_remove.includes(game.pilotName)) {
    failures.push("save shelf still showed the removed pilot");
  }
  if (report.console.length > 0) {
    failures.push(`browser console reported ${report.console.length} error(s)`);
  }
  if (report.archive_server_requests !== 1) {
    failures.push(`archive requested ${report.archive_server_requests} time(s), expected 1`);
  }
  if (failures.length > 0) {
    throw new Error(failures.join("; "));
  }
}

async function evaluate(page, expression, timeout) {
  const result = await page.send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
    timeout,
  });
  if (result.exceptionDetails) {
    throw new Error(JSON.stringify(result.exceptionDetails));
  }
  return result.result.value;
}

async function waitForChrome(port) {
  const deadline = Date.now() + 15_000;
  let lastError = null;
  while (Date.now() < deadline) {
    try {
      return await fetchJson(`http://127.0.0.1:${port}/json/version`);
    } catch (error) {
      lastError = error;
      await sleep(100);
    }
  }
  throw lastError ?? new Error("Chrome did not start");
}

function connect(webSocketUrl) {
  const ws = new WebSocket(webSocketUrl);
  let nextId = 1;
  const pending = new Map();
  const handlers = new Map();
  const ready = new Promise((resolve, reject) => {
    ws.addEventListener("open", resolve, { once: true });
    ws.addEventListener("error", reject, { once: true });
  });

  ws.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      const { resolve, reject } = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) {
        reject(new Error(JSON.stringify(message.error)));
      } else {
        resolve(message.result ?? {});
      }
      return;
    }

    const methodHandlers = handlers.get(message.method);
    if (methodHandlers) {
      for (const handler of methodHandlers) {
        handler(message.params ?? {});
      }
    }
  });

  return {
    ready,
    close: () => ws.close(),
    on(method, handler) {
      if (!handlers.has(method)) {
        handlers.set(method, []);
      }
      handlers.get(method).push(handler);
    },
    send(method, params = {}) {
      const id = nextId++;
      ws.send(JSON.stringify({ id, method, params }));
      return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
    },
  };
}

async function fetchJson(url) {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`${url} -> HTTP ${response.status}`);
  }
  return response.json();
}

function envNumber(name, fallback) {
  const raw = process.env[name];
  if (!raw) {
    return fallback;
  }
  const parsed = Number(raw);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new Error(`${name} must be a positive number, got ${JSON.stringify(raw)}`);
  }
  return parsed;
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
