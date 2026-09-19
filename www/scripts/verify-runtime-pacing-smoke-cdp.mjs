#!/usr/bin/env node

import { browserCases } from "./browser-cases.mjs";

import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(scriptDir, "..");
const sampleMs =
  process.env.SYSTEMLESS_RUNTIME_SMOKE_SAMPLE_MS ??
  process.env.SYSTEMLESS_RUNTIME_SAMPLE_MS ??
  "20000";
const attempts = Number(process.env.SYSTEMLESS_RUNTIME_SMOKE_ATTEMPTS ?? "2");
const verifier = join(scriptDir, "verify-runtime-pacing-cdp.mjs");

const games = browserCases();

const selectedIds = new Set(
  (process.env.SYSTEMLESS_RUNTIME_SMOKE_GAMES ?? games.map((game) => game.id).join(","))
    .split(",")
    .map((id) => id.trim())
    .filter(Boolean),
);
const selected = games.filter((game) => selectedIds.has(game.id));

for (const id of selectedIds) {
  if (!games.some((game) => game.id === id)) {
    throw new Error(`Unknown runtime pacing smoke game: ${id}`);
  }
}

const reports = [];
for (const game of selected) {
  if (!existsSync(game.archivePath)) {
    throw new Error(`Missing local archive for ${game.id}: ${game.archivePath}`);
  }

  process.stderr.write(`[runtime-pacing-smoke] ${game.id} ${game.route}\n`);
  const report = await runVerifierWithRetry(game);
  reports.push(summarizeReport(game, report));
}

console.log(JSON.stringify({ sample_ms: Number(sampleMs), games: reports }, null, 2));

async function runVerifierWithRetry(game) {
  let lastError;
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      return await runVerifier(game);
    } catch (error) {
      lastError = error;
      if (attempt < attempts) {
        process.stderr.write(
          `[runtime-pacing-smoke] ${game.id} attempt ${attempt} failed; retrying\n`,
        );
      }
    }
  }
  throw lastError;
}

function runVerifier(game) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [verifier], {
      cwd: repoRoot,
      env: {
        ...process.env,
        SYSTEMLESS_RUNTIME_ROUTE: game.route,
        SYSTEMLESS_RUNTIME_ARCHIVE_URL: game.archiveUrl,
        SYSTEMLESS_RUNTIME_ARCHIVE_PATH: game.archivePath,
        SYSTEMLESS_RUNTIME_SAMPLE_MS: sampleMs,
        SYSTEMLESS_MAX_LOADING_RAF_GAP_MS: String(game.maxLoadingRafGapMs),
        SYSTEMLESS_EXPECT_ARCHIVE_REQUESTS: String(game.expectedArchiveRequests),
        ...(game.minSteadyGuestMips
          ? { SYSTEMLESS_MIN_STEADY_GUEST_MIPS: String(game.minSteadyGuestMips) }
          : {}),
        ...(game.maxFirstRuntimeMs
          ? { SYSTEMLESS_MAX_FIRST_RUNTIME_MS: String(game.maxFirstRuntimeMs) }
          : {}),
      },
      stdio: ["ignore", "pipe", "pipe"],
    });

    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.on("error", reject);
    child.on("close", (code) => {
      if (code !== 0) {
        if (stdout.trim()) {
          process.stderr.write(stdout);
        }
        if (stderr.trim()) {
          process.stderr.write(stderr);
        }
        reject(new Error(`runtime pacing verifier failed for ${game.id}`));
        return;
      }

      try {
        resolve(JSON.parse(stdout));
      } catch (error) {
        reject(new Error(`Could not parse verifier output for ${game.id}: ${error.message}`));
      }
    });
  });
}

function summarizeReport(game, report) {
  return {
    id: game.id,
    route: report.route,
    first_runtime_ms: report.first_runtime_ms,
    first_runtime_limit_ms: game.maxFirstRuntimeMs ?? null,
    archive_server_requests: report.archive_server_requests,
    archive_request_limit: game.expectedArchiveRequests,
    max_loading_raf_gap_ms: report.max_loading_raf_gap_ms,
    loading_raf_gap_limit_ms: game.maxLoadingRafGapMs,
    largest_loading_gap: report.largest_loading_gaps?.[0] ?? null,
    max_runtime_raf_gap_ms: report.max_runtime_raf_gap_ms,
    max_runtime_raf_callback_ms: report.max_runtime_raf_callback_ms,
    max_runtime_frame_total_ms: report.max_runtime_frame_total_ms,
    max_runtime_frame_run_ms: report.max_runtime_frame_run_ms,
    min_runtime_audio_queue_ms: report.min_runtime_audio_queue_ms,
    avg_runtime_audio_queue_ms: report.avg_runtime_audio_queue_ms,
    steady_host_fps: report.steady_perf?.host_fps ?? null,
    steady_guest_mips: report.steady_perf?.guest_mips ?? null,
    steady_guest_mips_floor: game.minSteadyGuestMips ?? null,
    steady_guest_ticks_per_sec: report.steady_perf?.guest_ticks_per_sec ?? null,
  };
}
