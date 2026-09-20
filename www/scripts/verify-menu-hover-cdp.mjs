#!/usr/bin/env node

import { browserCases } from "./browser-cases.mjs";

import { spawn } from "node:child_process";
import { createReadStream, existsSync, statSync } from "node:fs";
import { createServer } from "node:http";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

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
const deviceScaleFactor = envNumber("SYSTEMLESS_MENU_HOVER_DPR", 1);
const hoverTimeoutMs = envNumber("SYSTEMLESS_MENU_HOVER_TIMEOUT_MS", 750);
const games = browserCases();
const selectedIds = new Set(
  (process.env.SYSTEMLESS_MENU_HOVER_GAMES ?? games.map(game => game.id).join(","))
    .split(",")
    .map((id) => id.trim())
    .filter(Boolean),
);



const selected = games.filter((game) => selectedIds.has(game.id));
for (const id of selectedIds) {
  if (!games.some((game) => game.id === id)) {
    throw new Error(`Unknown menu hover smoke game: ${id}`);
  }
}
if (!chromePath) {
  throw new Error("Set CHROME_BIN to a Chrome/Chromium executable for CDP verification");
}

const userDataDir = await mkdtemp(join(tmpdir(), "systemless-menu-hover-cdp-"));
const port = 9337 + Math.floor(Math.random() * 1000);
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
    "about:blank",
  ],
  { stdio: "ignore" },
);

try {
  const version = await waitForChrome(port);
  const reports = [];
  for (const game of selected) {
    process.stderr.write(`[menu-hover-smoke] ${game.id} ${game.route}\n`);
    reports.push(await verifyGame(version.webSocketDebuggerUrl, game));
  }
  console.log(JSON.stringify({ games: reports }, null, 2));
} finally {
  chrome.kill("SIGTERM");
  await sleep(250);
  await rm(userDataDir, { recursive: true, force: true });
}

async function verifyGame(browserWebSocketUrl, game) {
  const archiveServer = await serveArchive(game.archivePath);
  const browser = connect(browserWebSocketUrl);
  await browser.ready;
  const { targetId } = await browser.send("Target.createTarget", { url: "about:blank" });
  browser.close();

  const targets = await fetchJson(`http://127.0.0.1:${port}/json/list`);
  const target = targets.find((candidate) => candidate.id === targetId);
  if (!target) {
    throw new Error("Chrome target was not listed after creation");
  }

  const page = connect(target.webSocketDebuggerUrl);
  try {
    await page.ready;
    page.on("Fetch.requestPaused", (params) => handleArchiveRequest(page, params, game, archiveServer));
    await page.send("Page.enable");
    await page.send("Runtime.enable");
    await page.send("Page.addScriptToEvaluateOnNewDocument", {
      source: errorTracePrelude(),
    });
    await page.send("Fetch.enable", {
      patterns: [{ urlPattern: "https://assets.systemless.org/games/*", requestStage: "Request" }],
    });
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: 1280, height: 900, deviceScaleFactor, mobile: false,
    });
    await page.send("Page.navigate", { url: `${baseUrl}${game.route}` });

    await waitForCanvas(page, 20_000);
    if (game.preRuntimeHover) {
      await dispatchCanvasMouseMove(page, game.hoverPoint);
    }
    await waitForRuntime(page, game.id, 120_000);
    if (game.noticePixel) {
      await waitForPixel(page, game.noticePixel, 60_000, `${game.id} shareware notice`);
      await clickCanvasPoint(page, game.dismissPoint);
      await waitForPixelNot(page, game.noticePixel, 20_000, `${game.id} shareware dismissed`);
    }
    await waitForPixel(page, game.menuPixel, 60_000, `${game.id} menu ready`);

    const report = {
      id: game.id,
      route: game.route,
      archive_server_requests: archiveServer.requests(),
      archive_request_limit: game.expectedArchiveRequests,
      pre_runtime_hover: null,
      live_hover: null,
      console: await readConsole(page),
    };

    if (game.preRuntimeHover) {
      report.pre_runtime_hover = await assertPreRuntimeHover(page, game);
    }
    report.live_hover = await assertLiveHover(page, game);
    assertReport(game, report);
    if (game.id === "ev" && deviceScaleFactor > 1) {
      await clickCanvasPoint(page, { h: 275, v: 332 });
      const display = await waitFor(() => canvasInfo(page),
        (info) => info.output_scale > 1, 20_000, "high-density pilot dialog");
      const logicalWidth = display.width / display.output_scale;
      const expectedScale = Math.min(4, Math.max(1, Math.ceil(display.client_rect.width * deviceScaleFactor / logicalWidth)));
      if (display.output_scale !== expectedScale) throw new Error(`font output scale ${display.output_scale}, expected ${expectedScale}`);
      report.text_presentation = display;
    }
    return report;
  } finally {
    page.close();
    await archiveServer.close();
  }
}

async function assertPreRuntimeHover(page, game) {
  await captureRegion(page, game.diffRect);
  await dispatchCanvasMouseMove(page, game.neutralPoint);
  const diff = await waitForRegionDiff(page, game, [cursorRect(game.hoverPoint)]);
  return diff;
}

async function assertLiveHover(page, game) {
  await dispatchCanvasMouseMove(page, game.neutralPoint);
  await sleep(150);
  await captureRegion(page, game.diffRect);
  await dispatchCanvasMouseMove(page, game.hoverPoint);
  return await waitForRegionDiff(page, game, [cursorRect(game.hoverPoint)]);
}

async function waitForRegionDiff(page, game, excludedRects) {
  const started = Date.now();
  let last = null;
  while (Date.now() - started < hoverTimeoutMs) {
    last = await diffCapturedRegion(page, game.diffRect, excludedRects);
    if (last.changed_pixels >= game.minChangedPixels) {
      return {
        changed_pixels: last.changed_pixels,
        min_changed_pixels: game.minChangedPixels,
        elapsed_ms: Date.now() - started,
      };
    }
    await sleep(50);
  }
  throw new Error(
    `${game.id} menu hover changed ${last?.changed_pixels ?? 0} pixels, expected ` +
      `>= ${game.minChangedPixels} within ${hoverTimeoutMs}ms`,
  );
}

async function handleArchiveRequest(page, params, game, archiveServer) {
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

async function serveArchive(path) {
  const stat = statSync(path);
  let requests = 0;
  const server = createServer((req, res) => {
    if (req.url !== "/game.sit") {
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
      resolve(`http://127.0.0.1:${address.port}/game.sit`);
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

async function waitForCanvas(page, timeoutMs) {
  await waitFor(
    async () => await canvasInfo(page),
    (info) => info && info.client_rect.width > 0 && info.client_rect.height > 0,
    timeoutMs,
    "canvas visible",
  );
}

async function waitForRuntime(page, gameId, timeoutMs) {
  await waitFor(
    async () => await canvasInfo(page),
    (info) => info?.runtime_game_id === gameId,
    timeoutMs,
    `${gameId} runtime ready`,
  );
}

async function waitForPixel(page, pixel, timeoutMs, label) {
  await waitFor(
    async () => await readPixel(page, pixel.x, pixel.y),
    (rgb) => rgb && colorMatches(rgb, pixel),
    timeoutMs,
    label,
  );
}

async function waitForPixelNot(page, pixel, timeoutMs, label) {
  await waitFor(
    async () => await readPixel(page, pixel.x, pixel.y),
    (rgb) => rgb && !colorMatches(rgb, pixel),
    timeoutMs,
    label,
  );
}

async function waitFor(read, accept, timeoutMs, label) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    last = await read();
    if (accept(last)) {
      return last;
    }
    await sleep(100);
  }
  throw new Error(`Timed out waiting for ${label}: ${JSON.stringify(last)}`);
}

async function dispatchCanvasMouseMove(page, point) {
  const { x, y } = await clientPointForCanvasPoint(page, point);
  await page.send("Input.dispatchMouseEvent", {
    type: "mouseMoved",
    x,
    y,
    button: "none",
    buttons: 0,
  });
}

async function clickCanvasPoint(page, point) {
  const { x, y } = await clientPointForCanvasPoint(page, point);
  await page.send("Input.dispatchMouseEvent", {
    type: "mouseMoved",
    x,
    y,
    button: "none",
    buttons: 0,
  });
  await page.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x,
    y,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await sleep(80);
  await page.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x,
    y,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
}

async function clientPointForCanvasPoint(page, point) {
  return await evaluate(page, `(() => {
    const canvas = document.querySelector("canvas.game-canvas");
    if (!canvas) return null;
    const rect = canvas.getBoundingClientRect();
    return {
      x: rect.x + (${point.h} * rect.width * Number(canvas.dataset.outputScale || 1) / canvas.width),
      y: rect.y + (${point.v} * rect.height * Number(canvas.dataset.outputScale || 1) / canvas.height),
    };
  })()`);
}

async function canvasInfo(page) {
  return await evaluate(page, `(() => {
    const canvas = document.querySelector("canvas.game-canvas");
    const status = document.querySelector(".game-status");
    if (!canvas) return null;
    const rect = canvas.getBoundingClientRect();
    return {
      output_scale: Number(canvas.dataset.outputScale || 1),
      width: canvas.width,
      height: canvas.height,
      runtime_game_id: canvas.getAttribute("data-runtime-game-id"),
      status: status ? status.textContent : null,
      client_rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
    };
  })()`);
}

async function readPixel(page, x, y) {
  return await evaluate(page, `(() => {
    const canvas = document.querySelector("canvas.game-canvas");
    if (!canvas || ${x} >= canvas.width || ${y} >= canvas.height) return null;
    const data = window.__systemlessLogicalContext(canvas)
      .getImageData(${x}, ${y}, 1, 1).data;
    return [data[0], data[1], data[2]];
  })()`);
}

async function captureRegion(page, rect) {
  return await evaluate(page, `(() => {
    const canvas = document.querySelector("canvas.game-canvas");
    if (!canvas) return null;
    const x0 = Math.max(0, Math.min(canvas.width, ${rect.x0}));
    const y0 = Math.max(0, Math.min(canvas.height, ${rect.y0}));
    const x1 = Math.max(x0, Math.min(canvas.width, ${rect.x1}));
    const y1 = Math.max(y0, Math.min(canvas.height, ${rect.y1}));
    const width = x1 - x0;
    const height = y1 - y0;
    const data = window.__systemlessLogicalContext(canvas)
      .getImageData(x0, y0, width, height).data;
    window.__systemlessHoverBaseline = { x0, y0, width, height, data: Array.from(data) };
    return { x0, y0, width, height };
  })()`);
}

async function diffCapturedRegion(page, rect, excludedRects) {
  return await evaluate(page, `(() => {
    const baseline = window.__systemlessHoverBaseline;
    const canvas = document.querySelector("canvas.game-canvas");
    if (!baseline || !canvas) return { changed_pixels: 0 };
    const data = window.__systemlessLogicalContext(canvas)
      .getImageData(baseline.x0, baseline.y0, baseline.width, baseline.height).data;
    const excluded = ${JSON.stringify(excludedRects)};
    let changed = 0;
    for (let y = 0; y < baseline.height; y += 1) {
      const screenY = baseline.y0 + y;
      for (let x = 0; x < baseline.width; x += 1) {
        const screenX = baseline.x0 + x;
        if (excluded.some((rect) =>
          screenX >= rect.x0 && screenX < rect.x1 && screenY >= rect.y0 && screenY < rect.y1
        )) {
          continue;
        }
        const idx = (y * baseline.width + x) * 4;
        if (
          data[idx] !== baseline.data[idx] ||
          data[idx + 1] !== baseline.data[idx + 1] ||
          data[idx + 2] !== baseline.data[idx + 2]
        ) {
          changed += 1;
        }
      }
    }
    return {
      changed_pixels: changed,
      rect: ${JSON.stringify(rect)},
    };
  })()`);
}

async function readConsole(page) {
  return await evaluate(page, `window.__systemlessHoverConsole || []`);
}

function errorTracePrelude() {
  return `(() => {
    window.__systemlessLogicalContext = (canvas) => {
      const scale = Number(canvas.dataset.outputScale || 1);
      if (scale === 1) return canvas.getContext("2d", { willReadFrequently: true });
      const logical = document.createElement("canvas");
      logical.width = canvas.width / scale;
      logical.height = canvas.height / scale;
      const context = logical.getContext("2d", { willReadFrequently: true });
      context.imageSmoothingEnabled = false;
      context.drawImage(canvas, 0, 0, logical.width, logical.height);
      return context;
    };
    const messages = [];
    window.__systemlessHoverConsole = messages;
    window.addEventListener("error", (event) => {
      messages.push(String(event.message || event.error || "error"));
    });
    window.addEventListener("unhandledrejection", (event) => {
      messages.push(String(event.reason || "unhandled rejection"));
    });
  })();`;
}

function colorMatches(rgb, pixel) {
  const tolerance = pixel.tolerance ?? 0;
  return (
    Math.abs(rgb[0] - pixel.rgb[0]) <= tolerance &&
    Math.abs(rgb[1] - pixel.rgb[1]) <= tolerance &&
    Math.abs(rgb[2] - pixel.rgb[2]) <= tolerance
  );
}

function cursorRect(point) {
  return {
    x0: Math.max(0, point.h - 24),
    y0: Math.max(0, point.v - 24),
    x1: Math.max(0, point.h + 24),
    y1: Math.max(0, point.v + 24),
  };
}

function assertReport(game, report) {
  const failures = [];
  if (report.archive_server_requests !== game.expectedArchiveRequests) {
    failures.push(
      `archive requested ${report.archive_server_requests} time(s), expected ${game.expectedArchiveRequests}`,
    );
  }
  if (report.console.length > 0) {
    failures.push(`browser console reported ${report.console.length} error(s)`);
  }
  if (failures.length > 0) {
    throw new Error(`${game.id}: ${failures.join("; ")}`);
  }
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
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
