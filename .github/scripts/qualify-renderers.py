#!/usr/bin/env python3
"""Collect reproducible renderer qualification evidence; never approve rollout.

Run on a desktop session. Performance runs require an otherwise idle physical
GPU host. CI/software adapter success only establishes functional correctness.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--soak-seconds", type=int, default=300)
    parser.add_argument("--benchmarks", action="store_true")
    parser.add_argument("--skip-windows", action="store_true",
                        help="record window qualification as pending, e.g. a locked desktop")
    args = parser.parse_args()
    if not 0 <= args.soak_seconds <= 3600:
        parser.error("--soak-seconds must be 0..3600")
    root = Path(__file__).resolve().parents[2]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    def git(*arguments):
        return subprocess.check_output(["git", *arguments], cwd=root, text=True).strip()
    report = dict(schema_version=1, revision=git("rev-parse", "HEAD"),
                  dirty=git("status", "--porcelain").splitlines(),
                  host=platform.platform(), machine=platform.machine(),
                  fixture_sha256=hashlib.sha256((root / "tests/toolbox-showcase/toolbox-showcase.sit").read_bytes()).hexdigest(),
                  steps=[], default_eligible=False,
                  pending=["physical GPU/OS matrix", "real display pacing and input latency",
                           "sleep/wake, monitor/DPI migration and minimize/restore",
                           "long soak memory/power review", "unsafe Metal upload review",
                           "reviewed rollout decision"])
    def save():
        (output / "qualification.json").write_text(json.dumps(report, indent=2) + "\n")
    def run(name, command, timeout):
        step = dict(name=name, command=command, status="running")
        report["steps"].append(step)
        save()
        started = time.monotonic()
        with (output / (name + ".log")).open("w") as log:
            try:
                result = subprocess.run(command, cwd=root,
                                        env=dict(os.environ, SYSTEMLESS_REQUIRE_GPU="1"),
                                        stdout=log, stderr=subprocess.STDOUT, timeout=timeout)
                step.update(status="passed" if result.returncode == 0 else "failed",
                            returncode=result.returncode)
            except (subprocess.TimeoutExpired, OSError) as error:
                step.update(status="failed", error=str(error))
        step["seconds"] = time.monotonic() - started
        save()
        print(name + ": " + step["status"], flush=True)
        return step["status"] == "passed"
    save()
    if not run("gpu-tests", ["cargo", "test", "--locked", "--profile", "ci-test", "--bin",
                             "systemless", "--", "--test-threads=1"], 1800):
        return 1
    if args.skip_windows:
        report["pending"].append("native window lifecycle and recovery")
    else:
        if not run("probe-build", ["cargo", "build", "--locked", "--profile", "ci-test", "--bin",
                                   "systemless", "--features", "test-support"], 1800):
            return 1
        if not run("native-window", [sys.executable, str(root / ".github/scripts/verify-native-window.py"),
                                     "--output", str(output / "windows")], 2500):
            return 1
        if args.soak_seconds:
            if not run("lifecycle-soak", [sys.executable, str(root / ".github/scripts/verify-native-window.py"),
                                          "--soak-only", "--soak-seconds", str(args.soak_seconds),
                                          "--output", str(output / "soak")], 4 * (args.soak_seconds + 120) + 60):
                return 1
    if args.benchmarks:
        if not run("benchmarks", [sys.executable, str(root / ".github/scripts/benchmark-renderers.py"),
                                  "--output", str(output / "benchmarks")], 7200):
            return 1
    else:
        report["pending"].append("production release benchmarks")
    report["automated_checks_passed"] = True
    report["adapters"] = sorted({line.strip() for log in output.rglob("*.log")
                                  for line in log.read_text(errors="replace").splitlines()
                                  if line.startswith("[GPU] wgpu ") or line.startswith("[GPU] QD3D rasterization:")
                                  or line.startswith("[GPU] QuickDraw rasterization:")})
    save()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
