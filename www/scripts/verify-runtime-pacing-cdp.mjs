#!/usr/bin/env node

import { percentiles } from "./runtime-metrics.mjs";
import { spawn } from "node:child_process";
import { createReadStream, existsSync, statSync } from "node:fs";
import { createServer } from "node:http";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
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
const repoRoot = join(scriptDir, "..", "..");
const baseUrl = (process.env.SYSTEMLESS_ORG_URL ?? DEFAULT_BASE_URL).replace(/\/$/, "");
const route = process.env.SYSTEMLESS_RUNTIME_ROUTE;
const archiveUrl = process.env.SYSTEMLESS_RUNTIME_ARCHIVE_URL;
const archivePath = process.env.SYSTEMLESS_RUNTIME_ARCHIVE_PATH;
if (!route || !archiveUrl || !archivePath) {
  throw new Error("Set SYSTEMLESS_RUNTIME_ROUTE, SYSTEMLESS_RUNTIME_ARCHIVE_URL and SYSTEMLESS_RUNTIME_ARCHIVE_PATH from the catalogue entry under test");
}
const archiveRequestUrls = expectedArchiveRequestUrls(baseUrl, archiveUrl);
const gpuEnabled = process.env.SYSTEMLESS_RUNTIME_GPU === "1";
const sampleMs = envNumber("SYSTEMLESS_RUNTIME_SAMPLE_MS", 20_000);
const maxRuntimeRafGapMs = envNumber("SYSTEMLESS_MAX_RUNTIME_RAF_GAP_MS", 250);
const maxLoadingRafGapMs = envNumber("SYSTEMLESS_MAX_LOADING_RAF_GAP_MS", 500);
const maxFirstRuntimeMs = envOptionalNumber("SYSTEMLESS_MAX_FIRST_RUNTIME_MS");
const maxRuntimeRafCallbackMs = envNumber("SYSTEMLESS_MAX_RUNTIME_RAF_CALLBACK_MS", 50);
const maxRuntimeFrameTotalMs = envNumber("SYSTEMLESS_MAX_RUNTIME_FRAME_TOTAL_MS", 50);
const minRuntimeAudioQueueMs = envNumber("SYSTEMLESS_MIN_RUNTIME_AUDIO_QUEUE_MS", 40);
const minSteadyHostFps = envNumber("SYSTEMLESS_MIN_STEADY_HOST_FPS", 55);
const minSteadyGuestTicksPerSec = envNumber("SYSTEMLESS_MIN_STEADY_GUEST_TICKS_PER_SEC", 50);
const minSteadyGuestMips = envOptionalNumber("SYSTEMLESS_MIN_STEADY_GUEST_MIPS");
const targetGuestTick = envOptionalNumber("SYSTEMLESS_RUNTIME_TARGET_TICK");
const runtimeWarmupMs = envNumber("SYSTEMLESS_RUNTIME_WARMUP_MS", 1000);
const expectedArchiveRequests =
  envOptionalNumber("SYSTEMLESS_EXPECT_ARCHIVE_REQUESTS") ?? 1;
const chromePath = process.env.CHROME_BIN ?? DEFAULT_CHROME_PATHS.find(existsSync);

if (!chromePath) {
  throw new Error("Set CHROME_BIN to a Chrome/Chromium executable for CDP verification");
}

const archiveServer = await serveArchive(archivePath);
const userDataDir = await mkdtemp(join(tmpdir(), "systemless-runtime-cdp-"));
const port = 9337 + Math.floor(Math.random() * 1000);
const chrome = spawn(
  chromePath,
  [
    "--headless=new",
    `--remote-debugging-port=${port}`,
    `--user-data-dir=${userDataDir}`,
    ...(gpuEnabled ? [] : ["--disable-gpu"]),
    "--autoplay-policy=no-user-gesture-required",
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
  await page.send("Page.addScriptToEvaluateOnNewDocument", {
    source: `window.__systemlessProbePresentation = ${process.env.SYSTEMLESS_RUNTIME_PRESENTATION_DIAGNOSTICS === "1"};window.__systemlessProbeAudio = ${process.env.SYSTEMLESS_RUNTIME_AUDIO_DIAGNOSTICS === "1"};` + runtimeTracePrelude(),
  });
  await page.send("Fetch.enable", {
    patterns: [...archiveRequestUrls].map((url) => ({
      urlPattern: url,
      requestStage: "Request",
    })),
  });
  await page.send("Page.navigate", { url: `${baseUrl}${route}` });

  const probe = await evaluate(
    page,
    `(${runtimeProbe.toString()})(${JSON.stringify(sampleMs)}, ${process.env.SYSTEMLESS_RUNTIME_DEBUG !== "0"}, ${JSON.stringify(targetGuestTick ?? null)})`,
    sampleMs + 60_000,
  );
  if (process.env.SYSTEMLESS_RUNTIME_SCREENSHOT_PATH) {
    const screenshot = await page.send("Page.captureScreenshot", { format: "png" });
    await writeFile(process.env.SYSTEMLESS_RUNTIME_SCREENSHOT_PATH, Buffer.from(screenshot.data, "base64"));
  }
  page.close();

  const report = buildReport(
    probe.samples,
    probe.console,
    probe.raf_trace,
    probe.long_tasks,
    probe.frame_trace,
    probe.worker_trace,
    probe.started_at,
  );
  report.archive_server_requests = archiveServer.requests();
  report.environment = probe.environment;
  report.browser = version.Browser;
  report.progress_endpoint = probe.progress_endpoint;
  report.worker_startup = probe.worker_startup;
  report.audio_diagnostics = probe.audio_diagnostics;
  if (process.env.SYSTEMLESS_RUNTIME_PRESENTATION_DIAGNOSTICS === "1") {
    const cutoff = probe.started_at + (report.first_runtime_ms ?? Infinity) + runtimeWarmupMs;
    const images = probe.worker_trace.filter(entry => entry.t >= cutoff && entry.presentationMetrics?.completeImage);
    // Renderer and owner replies use separate channels. Correlate after capture
    // so a fast submission notice can precede the owner's metadata reply.
    for (const entry of probe.presentation_trace.filter(entry => entry.direct)) {
      const owner = probe.worker_trace.findLast(frame => frame.directSequence === entry.sequence
        && frame.rendererGeneration === entry.rendererGeneration);
      if (owner) {
        entry.ownerReceivedAt = owner.t;
        entry.requestToSubmitAckMs = entry.t - owner.requestedAt;
      }
    }
    const submitted = probe.presentation_trace.filter(entry => entry.ownerReceivedAt >= cutoff);
    report.presentation_diagnostics = {
      complete_images: images.length,
      packet_bytes: percentiles(images.map(entry => entry.packetBytes)),
      packet_kinds: [...new Set(images.map(entry => entry.packetKind))],
      owner_guest_ms: percentiles(images.map(entry => entry.presentationMetrics.guestMs)),
      owner_snapshot_ms: percentiles(images.map(entry => entry.presentationMetrics.snapshotMs)),
      wasm_to_js_packet_ms: percentiles(images.map(entry => entry.presentationMetrics.jsCopyMs)),
      host_receive_to_renderer_send_ms: percentiles(submitted.map(entry => entry.hostWaitMs)),
      renderer_roundtrip_ms: percentiles(submitted.map(entry => entry.rendererRoundtripMs)),
      renderer_submit_ms: percentiles(submitted.map(entry => entry.renderSubmitMs)),
      host_request_to_submit_ack_ms: percentiles(submitted.map(entry => entry.requestToSubmitAckMs)),
      submitted_packets: submitted.length,
      max_renderer_in_flight: submitted.some(entry => entry.direct) ? null : probe.max_renderer_in_flight,
      note: "Owner phases use its local clock; request/ack and host waits use the host clock, direct submissions are acknowledged directly by the renderer, without waiting for owner credit processing. Submission acknowledgements do not measure GPU completion or physical display. Diagnostics are separate from primary timing runs.",
    };
  }
  if (process.env.SYSTEMLESS_RUNTIME_TRACE_PATH) {
    await writeFile(process.env.SYSTEMLESS_RUNTIME_TRACE_PATH, JSON.stringify(probe));
  }
  console.log(JSON.stringify(report, null, 2));
  if (targetGuestTick != null && !probe.progress_endpoint.reached) {
    throw new Error(`Guest did not reach tick ${targetGuestTick} before timeout`);
  }
  assertRuntimePacing(report);
} finally {
  await archiveServer.close();
  chrome.kill("SIGTERM");
  await sleep(250);
  await rmWithRetry(userDataDir);
}

async function rmWithRetry(path) {
  let lastError;
  for (let attempt = 0; attempt < 10; attempt += 1) {
    try {
      await rm(path, { recursive: true, force: true });
      return;
    } catch (error) {
      lastError = error;
      if (!["EBUSY", "ENOTEMPTY", "EPERM"].includes(error?.code)) {
        throw error;
      }
      await sleep(250);
    }
  }
  throw lastError;
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

async function handleArchiveRequest(page, params) {
  const url = params.request?.url ?? "";
  if (!archiveRequestUrls.has(url)) {
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

function expectedArchiveRequestUrls(baseUrl, archiveUrl) {
  const urls = new Set([archiveUrl]);
  const r2Prefix = "https://assets.systemless.org/games/";
  if (archiveUrl.startsWith(r2Prefix) && isLocalBaseUrl(baseUrl)) {
    urls.add(`${baseUrl}/assets/games/${archiveUrl.slice(r2Prefix.length)}`);
  }
  return urls;
}

function isLocalBaseUrl(baseUrl) {
  try {
    const { hostname } = new URL(baseUrl);
    return hostname === "localhost" || hostname === "127.0.0.1" || hostname === "::1";
  } catch (_) {
    return false;
  }
}

async function runtimeProbe(sampleMs, showDebug, targetGuestTick) {
  const samples = [];
  const console = [];
  const startedAt = performance.now();
  let lastFrameTimestamp = startedAt;
  let debugEnabled = false;

  window.addEventListener("error", (event) => {
    console.push(String(event.error?.stack || event.message || event.error || "error"));
  });
  window.addEventListener("unhandledrejection", (event) => {
    console.push(String(event.reason || "unhandled rejection"));
  });

  return await new Promise((resolve) => {
    function tick(frameTimestamp) {
      const t = performance.now() - startedAt;
      const canvas = document.querySelector("canvas.game-canvas");
      const status = document.querySelector(".game-status");

      if (canvas && showDebug && !debugEnabled) {
        canvas.focus();
        canvas.dispatchEvent(
          new KeyboardEvent("keydown", { key: "F3", code: "F3", bubbles: true }),
        );
        debugEnabled = true;
      }

      samples.push({
        t,
        dt: frameTimestamp - lastFrameTimestamp,
        status: status ? status.textContent : null,
        runtime: canvas ? canvas.getAttribute("data-runtime-game-id") : null,
        worker: canvas?.getAttribute("data-runtime-worker") === "true",
      });
      lastFrameTimestamp = frameTimestamp;

      const latestFrame = (canvas?.getAttribute("data-runtime-worker") === "true"
        ? window.__systemlessWorkerTrace : window.__systemlessFrameTrace)?.at(-1);
      const reached = targetGuestTick !== null && latestFrame?.guestTick >= targetGuestTick;
      if (t < sampleMs && !reached) {
        requestAnimationFrame(tick);
      } else {
        resolve({
          samples,
          progress_endpoint: {
            requested_tick: targetGuestTick,
            reached,
            observed_tick: latestFrame?.guestTick ?? null,
            observed_instructions: latestFrame?.totalInstructions ?? null,
          },
          environment: {
            debug_overlay: showDebug,
            user_agent: navigator.userAgent,
            device_pixel_ratio: devicePixelRatio,
            visibility: document.visibilityState,
            renderer: document.querySelector("canvas.game-canvas")?.getAttribute("data-render-backend"),
            transport: document.querySelector("canvas.game-canvas")?.getAttribute("data-render-transport"),
            canvas_width: document.querySelector("canvas.game-canvas")?.width,
            canvas_height: document.querySelector("canvas.game-canvas")?.height,
            cpu_mhz: document.querySelector("canvas.game-canvas")?.getAttribute("data-runtime-cpu-mhz"),
            output_scale: document.querySelector("canvas.game-canvas")?.getAttribute("data-output-scale"),
          },
          console,
          started_at: startedAt,
          raf_trace: window.__systemlessRafTrace || [],
          long_tasks: window.__systemlessLongTasks || [],
          frame_trace: window.__systemlessFrameTrace || [],
          worker_trace: window.__systemlessWorkerTrace || [],
          worker_startup: window.__systemlessWorkerStartup || [],
          audio_diagnostics: window.__systemlessAudioDiagnostics || [],
          presentation_trace: window.__systemlessPresentationTrace || [],
          max_renderer_in_flight: window.__systemlessRendererInFlightMax || 0,
        });
      }
    }

    requestAnimationFrame(tick);
  });
}

function runtimeTracePrelude() {
  return `(() => {
    const originalRequestAnimationFrame = window.requestAnimationFrame.bind(window);
    const rafTrace = [];
    const frameTrace = [];
    const workerTrace = [];
    const longTasks = [];
    let nextRafId = 0;
    window.__systemlessRafTrace = rafTrace;
    window.__systemlessFrameTrace = frameTrace;
    window.__systemlessWorkerTrace = workerTrace;
    window.__systemlessLongTasks = longTasks;
    const presentation = window.__systemlessPresentationTrace = [];
    const ownedPackets = new WeakMap();
    window.__systemlessRendererInFlightMax = 0;
    const startup = window.__systemlessWorkerStartup = [];
    const audio = window.__systemlessAudioDiagnostics = [];
    if (window.__systemlessProbeAudio && window.AudioWorkletNode) {
      const NativeAudioWorkletNode = window.AudioWorkletNode;
      window.AudioWorkletNode = new Proxy(NativeAudioWorkletNode, {
        construct(Target, args) {
          const node = Reflect.construct(Target, args);
          node.port.addEventListener("message", event => {
            if (event.data?.type !== "diagnostics") return;
            audio.push({ t: performance.now(), ...event.data });
            if (audio.length > 6000) audio.splice(0, audio.length - 6000);
          });
          node.port.start();
          node.port.postMessage({ type: "diagnostics", enabled: true });
          return node;
        },
      });
    }
    const NativeWorker = window.Worker;
    window.Worker = new Proxy(NativeWorker, {
      construct(Target, args) {
        const worker = Reflect.construct(Target, args);
        const postMessage = worker.postMessage.bind(worker);
        let frameSentAt = null;
        const isRenderer = String(args[0]).includes("renderer-worker");
        const submissions = new Map();
        worker.postMessage = (...messageArgs) => {
          if (messageArgs[0]?.type === "frame") {
            frameSentAt = performance.now();
            if (window.__systemlessProbePresentation) {
              const message = messageArgs[0];
              if (isRenderer) {
                const buffer = message.compact?.cells?.buffer ?? message.pixels?.buffer;
                const owner = buffer && ownedPackets.get(buffer);
                if (owner) {
                  const views = [message.pixels, message.palette, message.cursor?.pixels, message.compact?.cells, message.compact?.detail].filter(Boolean);
                  submissions.set(message.sequence, { ...owner, sentAt: frameSentAt, kind: message.kind,
                    bytes: views.reduce((total, view) => total + view.byteLength, 0) });
                  window.__systemlessRendererInFlightMax = Math.max(window.__systemlessRendererInFlightMax, submissions.size);
                }
              } else message.measurePresentation = true;
            }
          }
          if (window.__systemlessProbeAudio && messageArgs[0]?.type === "boot") {
            startup.push({ t: performance.now(), type: "boot" });
          }
          return postMessage(...messageArgs);
        };
        worker.addEventListener("message", (event) => {
          const data = event.data;
          if (window.__systemlessProbeAudio && ["progress", "ready"].includes(data?.type)) {
            startup.push({ t: performance.now(), type: data.type, progress: data.progress });
            if (startup.length > 200) startup.shift();
          }
          if (window.__systemlessProbePresentation && isRenderer && ["submitted", "dropped"].includes(data?.type)) {
            const sent = submissions.get(data.sequence);
            submissions.delete(data.sequence);
            if (sent && data.type === "submitted") {
              const now = performance.now();
              presentation.push({ ...sent, t: now, sequence: data.sequence,
                hostWaitMs: sent.sentAt - sent.ownerReceivedAt,
                rendererRoundtripMs: now - sent.sentAt, renderSubmitMs: data.renderMs,
                requestToSubmitAckMs: now - sent.requestedAt });
              if (presentation.length > 6000) presentation.splice(0, presentation.length - 6000);
            }
          }
          if (window.__systemlessProbePresentation && isRenderer && data?.type === "directSubmitted") {
            presentation.push({ t: performance.now(), sequence: data.sequence, rendererGeneration: data.rendererGeneration,
              kind: data.kind, bytes: data.bytes, direct: true, renderSubmitMs: data.renderMs });
            if (presentation.length > 6000) presentation.splice(0, presentation.length - 6000);
          }
          if (data?.type !== "frame") return;
          const t = performance.now();
          if (window.__systemlessProbePresentation) {
            const buffer = data.compactFrame?.compact.cells.buffer ?? data.indexedFrame?.pixels.buffer ?? data.frame?.buffer;
            if (buffer) ownedPackets.set(buffer, { requestedAt: frameSentAt, ownerReceivedAt: t, guestTick: data.guestTick });
          }
          workerTrace.push({
            t,
            // Includes worker execution, message transfer, and scheduling.
            totalMs: frameSentAt === null ? null : t - frameSentAt,
            guestTick: data.guestTick,
            totalInstructions: data.totalInstructions,
            ticksBehind: data.ticksBehind,
            lastSteps: data.lastSteps,
            cpuBudgetMs: data.cpuBudgetMs,
            audioQueueMs: data.audioQueueMs,
            presentationMetrics: data.presentationMetrics,
            ...(data.directFrame ? { directSequence: data.directFrame.sequence, rendererGeneration: data.directFrame.rendererGeneration, requestedAt: frameSentAt } : {}),
            packetKind: data.directFrame?.kind ?? (data.compactFrame ? "compact" : data.indexedFrame ? "indexed8" : data.frame ? "rgba" : null),
            packetBytes: data.directFrame?.bytes ?? [data.frame, data.indexedFrame?.pixels, data.indexedFrame?.palette,
              data.indexedFrame?.cursor?.pixels, data.compactFrame?.compact.cells,
              data.compactFrame?.compact.detail].filter(Boolean).reduce((bytes, view) => bytes + view.byteLength, 0),
            visualWork: data.visualWork,
            painted: !!(data.frame || data.gpuFrame || data.indexedFrame || data.compactFrame || data.directFrame),
          });
          if (workerTrace.length > 6000) workerTrace.splice(0, workerTrace.length - 6000);
        });
        return worker;
      },
    });
    window.requestAnimationFrame = (callback) => {
      const id = ++nextRafId;
      const scheduledAt = performance.now();
      return originalRequestAnimationFrame((timestamp) => {
        const startedAt = performance.now();
        let result;
        try {
          result = callback(timestamp);
        } finally {
          const endedAt = performance.now();
          rafTrace.push({
            id,
            scheduledAt,
            frameTimestamp: timestamp,
            startedAt,
            endedAt,
            duration: endedAt - startedAt,
            delayFromSchedule: startedAt - scheduledAt,
            callbackName: callback && callback.name ? callback.name : "",
            callbackSource: String(callback).slice(0, 120),
          });
          if (rafTrace.length > 6000) {
            rafTrace.splice(0, rafTrace.length - 6000);
          }
        }
        return result;
      });
    };
    try {
      new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) {
          longTasks.push({
            name: entry.name,
            startTime: entry.startTime,
            duration: entry.duration,
          });
        }
        if (longTasks.length > 1000) {
          longTasks.splice(0, longTasks.length - 1000);
        }
      }).observe({ type: "longtask", buffered: true });
    } catch (_) {
      // Long Tasks are unavailable in some browser modes.
    }
  })();`;
}

function buildReport(samples, console, rafTrace, longTasks, frameTrace, workerTrace, probeStartedAt) {
  const gaps = samples.filter((sample) => Number.isFinite(sample.dt) && sample.dt > 50);
  const runtimeStart = samples.find((sample) => sample.runtime)?.t ?? null;
  const runtimeStartedAt = runtimeStart === null ? null : probeStartedAt + runtimeStart;
  const measuredRuntimeStartedAt = runtimeStartedAt === null
    ? null
    : runtimeStartedAt + runtimeWarmupMs;
  const measuredRuntimeStart = runtimeStart === null ? null : runtimeStart + runtimeWarmupMs;
  const runtimeGaps = measuredRuntimeStart === null
    ? []
    : gaps.filter((sample) => sample.runtime && sample.t >= measuredRuntimeStart);
  const runtimeTrace = measuredRuntimeStartedAt === null
    ? []
    : rafTrace.filter((entry) => entry.startedAt >= measuredRuntimeStartedAt);
  const loadingGaps = runtimeStart === null
    ? gaps
    : gaps.filter((sample) => sample.t < runtimeStart);
  const runtimeLongTasks = measuredRuntimeStartedAt === null
    ? []
    : longTasks.filter((entry) => entry.startTime >= measuredRuntimeStartedAt);
  const worker = samples.some((sample) => sample.worker);
  const allFrames = worker ? workerTrace : frameTrace;
  const runtimeFrames = measuredRuntimeStartedAt === null
    ? []
    : allFrames.filter((entry) => entry.t >= measuredRuntimeStartedAt);
  const runtimeCpuBudgets = runtimeFrames
    .map((entry) => entry.cpuBudgetMs)
    .filter(Number.isFinite);
  const runtimeAudioQueues = runtimeFrames
    .map((entry) => entry.audioQueueMs)
    .filter(Number.isFinite);
  const longRuntimeRafCallbacks = runtimeTrace
    .filter((entry) => Number.isFinite(entry.duration) && entry.duration > 25)
    .map((entry) => ({
      t: round1(entry.startedAt - runtimeStartedAt),
      duration: round1(entry.duration),
      delay_from_schedule: round1(entry.delayFromSchedule),
      callback_name: entry.callbackName,
      callback_source: entry.callbackSource,
    }));
  const largestRuntimeRafCallbacks = longRuntimeRafCallbacks
    .sort((a, b) => b.duration - a.duration)
    .slice(0, 8);
  const steadyPerf = steadyPerfFromFrameTrace(runtimeFrames, runtimeStartedAt);

  return {
    route,
    gpu_requested: gpuEnabled,
    runtime_raf_gap_ms: percentiles(samples.filter((sample) => sample.runtime && sample.t >= measuredRuntimeStart).map((sample) => sample.dt)),
    runtime_raf_callback_ms: percentiles(runtimeTrace.map((entry) => entry.duration)),
    runtime_frame_total_ms: percentiles(runtimeFrames.map((entry) => entry.totalMs)),
    runtime_frame_run_ms: percentiles(runtimeFrames.map((entry) => entry.runMs)),
    runtime_frame_render_ms: percentiles(runtimeFrames.map((entry) => entry.renderMs)),
    runtime_frame_paint_ms: percentiles(runtimeFrames.map((entry) => entry.paintMs)),
    guest_progress: {
      first_tick: runtimeFrames[0]?.guestTick ?? null,
      last_tick: runtimeFrames.at(-1)?.guestTick ?? null,
      first_instructions: runtimeFrames[0]?.totalInstructions ?? null,
      last_instructions: runtimeFrames.at(-1)?.totalInstructions ?? null,
    },
    runtime_mode: worker ? "worker" : "main-thread",
    archive_url: archiveUrl,
    archive_path: archivePath,
    sample_ms: sampleMs,
    runtime_warmup_ms: runtimeWarmupMs,
    sample_count: samples.length,
    status_changes: statusChanges(samples),
    first_runtime_ms: samples.find((sample) => sample.runtime)?.t ?? null,
    max_raf_gap_ms: max(samples.map((sample) => sample.dt)),
    max_loading_raf_gap_ms: max(loadingGaps.map((sample) => sample.dt)),
    loading_raf_gaps_over_50_ms: loadingGaps.length,
    max_runtime_raf_gap_ms: max(runtimeGaps.map((sample) => sample.dt)),
    runtime_raf_gaps_over_50_ms: runtimeGaps.length,
    max_runtime_raf_callback_ms: max(runtimeTrace.map((entry) => entry.duration)),
    runtime_raf_callbacks_over_25_ms: longRuntimeRafCallbacks.length,
    largest_runtime_raf_callbacks: largestRuntimeRafCallbacks,
    max_runtime_frame_total_ms: max(runtimeFrames.map((entry) => entry.totalMs)),
    max_runtime_frame_run_ms: max(runtimeFrames.map((entry) => entry.runMs)),
    max_runtime_frame_render_ms: max(runtimeFrames.map((entry) => entry.renderMs)),
    max_runtime_frame_paint_ms: max(runtimeFrames.map((entry) => entry.paintMs)),
    max_runtime_cpu_budget_ms: max(runtimeCpuBudgets),
    runtime_frames_at_healthy_cpu_budget: runtimeCpuBudgets.filter((value) => value >= 14).length,
    min_runtime_audio_queue_ms: min(runtimeAudioQueues),
    avg_runtime_audio_queue_ms: avg(runtimeAudioQueues),
    max_runtime_audio_queue_ms: max(runtimeAudioQueues),
    runtime_audio_queue_samples: runtimeAudioQueues.length,
    largest_runtime_frames: runtimeFrames
      .map((entry) => ({
        t: round1(entry.t - runtimeStartedAt),
        total_ms: round1(entry.totalMs),
        run_ms: round1(entry.runMs),
        render_ms: round1(entry.renderMs),
        paint_ms: round1(entry.paintMs),
        guest_tick: entry.guestTick,
        ticks_behind: entry.ticksBehind,
        last_steps: entry.lastSteps,
        cpu_budget_ms: maybeRound1(entry.cpuBudgetMs),
        audio_queue_ms: maybeRound1(entry.audioQueueMs),
        visual_work: entry.visualWork,
        size_changed: entry.sizeChanged,
        painted: entry.painted,
      }))
      .sort((a, b) => b.total_ms - a.total_ms)
      .slice(0, 8),
    max_runtime_long_task_ms: max(runtimeLongTasks.map((entry) => entry.duration)),
    runtime_long_tasks_over_50_ms: runtimeLongTasks.filter((entry) => entry.duration > 50).length,
    largest_runtime_long_tasks: runtimeLongTasks
      .map((entry) => ({
        t: runtimeStartedAt === null ? null : round1(entry.startTime - runtimeStartedAt),
        duration: round1(entry.duration),
        name: entry.name,
      }))
      .sort((a, b) => b.duration - a.duration)
      .slice(0, 8),
    largest_runtime_gaps: runtimeGaps
      .map((sample) => ({
        t: round1(sample.t),
        dt: round1(sample.dt),
      }))
      .sort((a, b) => b.dt - a.dt)
      .slice(0, 8),
    largest_loading_gaps: loadingGaps
      .map((sample) => ({
        t: round1(sample.t),
        dt: round1(sample.dt),
        status: sample.status,
      }))
      .sort((a, b) => b.dt - a.dt)
      .slice(0, 8),
    steady_perf: steadyPerf
      ? {
          t: round1(steadyPerf.t),
          host_fps: steadyPerf.hostFps,
          frame_ms: steadyPerf.frameMs,
          guest_mips: steadyPerf.guestMips,
          guest_ticks_per_sec: steadyPerf.guestTicksPerSec,
          tick_debt: steadyPerf.tickDebt,
          slice_instructions: steadyPerf.sliceInstructions,
        }
      : null,
    console,
  };
}

function assertRuntimePacing(report) {
  const failures = [];
  if (report.first_runtime_ms === null) {
    failures.push("runtime canvas never reported a loaded game");
  } else if (maxFirstRuntimeMs !== null && report.first_runtime_ms > maxFirstRuntimeMs) {
    failures.push(
      `first runtime ${round1(report.first_runtime_ms)}ms exceeded ${maxFirstRuntimeMs}ms`,
    );
  }
  if (report.max_runtime_cpu_budget_ms <= 0) {
    failures.push("runtime frame trace did not report CPU budget diagnostics");
  }
  if (report.runtime_audio_queue_samples <= 0) {
    failures.push("runtime frame trace did not report audio queue diagnostics");
  } else if (report.min_runtime_audio_queue_ms < minRuntimeAudioQueueMs) {
    failures.push(
      `runtime audio queue ${round1(report.min_runtime_audio_queue_ms)}ms below ${minRuntimeAudioQueueMs}ms`,
    );
  }
  if (report.max_runtime_raf_gap_ms > maxRuntimeRafGapMs) {
    failures.push(
      `runtime rAF gap ${round1(report.max_runtime_raf_gap_ms)}ms exceeded ${maxRuntimeRafGapMs}ms`,
    );
  }
  if (report.max_loading_raf_gap_ms > maxLoadingRafGapMs) {
    failures.push(
      `loading rAF gap ${round1(report.max_loading_raf_gap_ms)}ms exceeded ${maxLoadingRafGapMs}ms`,
    );
  }
  if (report.max_runtime_raf_callback_ms > maxRuntimeRafCallbackMs) {
    failures.push(
      `runtime rAF callback ${round1(report.max_runtime_raf_callback_ms)}ms exceeded ${maxRuntimeRafCallbackMs}ms`,
    );
  }
  if (report.max_runtime_frame_total_ms > maxRuntimeFrameTotalMs) {
    failures.push(
      `runtime frame ${round1(report.max_runtime_frame_total_ms)}ms exceeded ${maxRuntimeFrameTotalMs}ms`,
    );
  }
  if (!report.steady_perf) {
    failures.push("no steady frame-trace sample was collected");
  } else {
    if (report.steady_perf.host_fps < minSteadyHostFps) {
      failures.push(
        `steady host FPS ${report.steady_perf.host_fps} below ${minSteadyHostFps}`,
      );
    }
    if (report.steady_perf.guest_ticks_per_sec < minSteadyGuestTicksPerSec) {
      failures.push(
        `steady guest ticks/sec ${report.steady_perf.guest_ticks_per_sec} below ${minSteadyGuestTicksPerSec}`,
      );
    }
    if (
      minSteadyGuestMips !== null
      && report.steady_perf.guest_mips < minSteadyGuestMips
    ) {
      failures.push(
        `steady guest MIPS ${report.steady_perf.guest_mips} below ${minSteadyGuestMips}`,
      );
    }
  }
  if (report.console.length > 0) {
    failures.push(`browser console reported ${report.console.length} error(s)`);
  }
  if (report.archive_server_requests !== expectedArchiveRequests) {
    failures.push(
      `archive requested ${report.archive_server_requests} time(s), expected ${expectedArchiveRequests}`,
    );
  }
  if (failures.length > 0) {
    throw new Error(failures.join("; "));
  }
}

function steadyPerfFromFrameTrace(runtimeFrames, runtimeStartedAt) {
  if (runtimeStartedAt === null || runtimeFrames.length < 2) {
    return null;
  }
  const tail = runtimeFrames
    .filter((entry) => Number.isFinite(entry.t) && Number.isFinite(entry.guestTick))
    .slice(-60);
  if (tail.length < 2) {
    return null;
  }
  const first = tail[0];
  const last = tail.at(-1);
  const elapsedMs = last.t - first.t;
  if (!Number.isFinite(elapsedMs) || elapsedMs <= 0) {
    return null;
  }
  const frameTotals = tail.map((entry) => entry.totalMs).filter(Number.isFinite);
  const guestTicks = Math.max(0, last.guestTick - first.guestTick);
  const instructions = tail
    .map((entry) => entry.lastSteps)
    .filter(Number.isFinite)
    .reduce((sum, steps) => sum + steps, 0);
  return {
    t: last.t - runtimeStartedAt,
    hostFps: ((tail.length - 1) * 1000) / elapsedMs,
    frameMs: avg(frameTotals),
    guestMips: instructions / elapsedMs / 1000,
    guestTicksPerSec: guestTicks * 1000 / elapsedMs,
    tickDebt: Number.isFinite(last.ticksBehind) ? last.ticksBehind : null,
    sliceInstructions: Number.isFinite(last.lastSteps) ? last.lastSteps : null,
  };
}

function statusChanges(samples) {
  const changes = [];
  let last;
  for (const sample of samples) {
    if (sample.status !== last) {
      changes.push({ t: round1(sample.t), status: sample.status });
      last = sample.status;
    }
  }
  return changes;
}

function max(values) {
  const finite = values.filter(Number.isFinite);
  return finite.length > 0 ? round1(Math.max(...finite)) : 0;
}

function min(values) {
  const finite = values.filter(Number.isFinite);
  return finite.length > 0 ? round1(Math.min(...finite)) : 0;
}

function avg(values) {
  const finite = values.filter(Number.isFinite);
  if (finite.length === 0) {
    return 0;
  }
  return round1(finite.reduce((sum, value) => sum + value, 0) / finite.length);
}

function maybeRound1(value) {
  return Number.isFinite(value) ? round1(value) : null;
}

function round1(value) {
  return Math.round(value * 10) / 10;
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

function envOptionalNumber(name) {
  const raw = process.env[name];
  if (!raw) {
    return null;
  }
  const parsed = Number(raw);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new Error(`${name} must be a positive number, got ${JSON.stringify(raw)}`);
  }
  return parsed;
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

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
