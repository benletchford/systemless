#!/usr/bin/env node

import { browserCases } from "./browser-cases.mjs";

import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdtemp, readFile, rm } from "node:fs/promises";
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
const baseUrl = (process.env.SYSTEMLESS_ORG_URL ?? DEFAULT_BASE_URL).replace(/\/$/, "");
const chromePath = process.env.CHROME_BIN ?? DEFAULT_CHROME_PATHS.find(existsSync);

if (!chromePath) {
  throw new Error("Set CHROME_BIN to a Chrome/Chromium executable for CDP verification");
}

const cases = browserCases();
const localArchives = new Map(cases.filter(c => c.archiveUrl && c.archivePath).map(c => [c.archiveUrl, c.archivePath]));

const archiveBodies = new Map();
for (const [url, path] of localArchives) {
  const bytes = await readFile(path);
  archiveBodies.set(url, {
    length: bytes.length,
    body: bytes.toString("base64"),
  });
}

const userDataDir = await mkdtemp(join(tmpdir(), "systemless-cdp-"));
const port = 9337 + Math.floor(Math.random() * 1000);
const chrome = spawn(
  chromePath,
  [
    "--headless=new",
    `--remote-debugging-port=${port}`,
    `--user-data-dir=${userDataDir}`,
    "--disable-gpu",
    "--no-first-run",
    "--no-default-browser-check",
    "about:blank",
  ],
  { stdio: "ignore" },
);

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

  const page = connect(target.webSocketDebuggerUrl);
  await page.ready;
  page.on("Fetch.requestPaused", (params) => handleArchiveRequest(page, params));
  await page.send("Page.enable");
  await page.send("Runtime.enable");
  await page.send("Fetch.enable", {
    patterns: [...localArchives.keys()].map(urlPattern => ({ urlPattern, requestStage: "Request" })),
  });

  const reports = [];
  for (const c of cases) {
    const policy = await readCanvasPolicy(page, c.route, { requireRuntime: c.requireRuntime ?? true });
    assertPolicy(policy, c.id, String(c.arrowsAsNumpad), c.requireRuntime ?? true);
    reports.push(policy);
  }
  console.log(JSON.stringify(reports, null, 2));
  page.close();
} finally {
  chrome.kill("SIGTERM");
  await sleep(250);
  await rm(userDataDir, { recursive: true, force: true });
}

async function handleArchiveRequest(page, params) {
  const url = params.request?.url ?? "";
  const archive = archiveBodies.get(url);
  if (!archive) {
    await page.send("Fetch.failRequest", {
      requestId: params.requestId,
      errorReason: "BlockedByClient",
    });
    return;
  }

  await page.send("Fetch.fulfillRequest", {
    requestId: params.requestId,
    responseCode: 200,
    responseHeaders: [
      { name: "access-control-allow-origin", value: "*" },
      { name: "content-type", value: "application/octet-stream" },
      { name: "content-length", value: String(archive.length) },
    ],
    body: archive.body,
  });
}

async function readCanvasPolicy(page, path, { requireRuntime }) {
  await page.send("Page.navigate", { url: `${baseUrl}${path}` });
  const deadline = Date.now() + (requireRuntime ? 120_000 : 20_000);
  let last = null;

  while (Date.now() < deadline) {
    last = await evaluate(page, `(() => {
      const el = document.querySelector("canvas.game-canvas");
      const status = document.querySelector(".game-status");
      return el ? {
        url: location.pathname,
        gameId: el.getAttribute("data-game-id"),
        arrowsAsNumpad: el.getAttribute("data-arrows-as-numpad"),
        runtimeGameId: el.getAttribute("data-runtime-game-id"),
        runtimeArrowsAsNumpad: el.getAttribute("data-runtime-arrows-as-numpad"),
        status: status ? status.textContent : null,
      } : null;
    })()`);

    if (last && (!requireRuntime || last.runtimeArrowsAsNumpad)) {
      return last;
    }
    if (last?.status?.startsWith("Boot failed")) {
      break;
    }
    await sleep(250);
  }

  throw new Error(`Timed out reading keyboard policy for ${path}: ${JSON.stringify(last)}`);
}

function assertPolicy(policy, gameId, arrowsAsNumpad, requireRuntime) {
  if (policy.gameId !== gameId || policy.arrowsAsNumpad !== arrowsAsNumpad) {
    throw new Error(
      `${gameId} configured policy mismatch: expected ${arrowsAsNumpad}, got ${JSON.stringify(policy)}`,
    );
  }
  if (
    requireRuntime &&
    (policy.runtimeGameId !== gameId || policy.runtimeArrowsAsNumpad !== arrowsAsNumpad)
  ) {
    throw new Error(
      `${gameId} runtime policy mismatch: expected ${arrowsAsNumpad}, got ${JSON.stringify(policy)}`,
    );
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

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
