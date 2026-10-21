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


def native_matrix_timeout(script, arguments, seconds=0):
    """Allow every selected case its own bounded execution and cleanup time."""
    cases = json.loads(subprocess.check_output(
        [sys.executable, str(script), "--dry-run", *arguments], text=True))
    return len(cases) * (seconds + 120) + 60


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--soak-seconds", type=int, default=300)
    parser.add_argument("--compute-soak-seconds", type=int, default=0,
                        help="active guest compute seconds per architecture; 0 skips, multi-hour runs are opt-in")
    parser.add_argument("--soak-runtime", choices=("all", "inline", "thread"), default="all")
    parser.add_argument("--soak-presenter", choices=("all", "platform", "wgpu", "software"), default="all")
    parser.add_argument("--benchmarks", action="store_true")
    parser.add_argument("--skip-windows", action="store_true",
                        help="record window qualification as pending, e.g. a locked desktop")
    args = parser.parse_args()
    if not 0 <= args.compute_soak_seconds <= 21600:
        parser.error("--compute-soak-seconds must be 0..21600")
    if not 0 <= args.soak_seconds <= 21600:
        parser.error("--soak-seconds must be 0..21600")
    if args.soak_seconds > 3600 and (args.soak_runtime == "all" or args.soak_presenter == "all"):
        parser.error("longer-than-one-hour soaks require --soak-runtime and --soak-presenter")
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
    if not run("guest-build", ["cargo", "build", "--locked", "--profile", "ci-test", "--bin",
                               "systemless"], 1800):
        return 1
    binary = root / "target/ci-test" / ("systemless.exe" if os.name == "nt" else "systemless")
    if not run("guest-pixels", [sys.executable, str(root / ".github/scripts/verify-gpu-play.py"),
                                "--binary", str(binary), "--output", str(output / "guest-pixels")], 1200):
        return 1
    if args.skip_windows:
        report["pending"].append("native window lifecycle and recovery")
    else:
        if not run("resource-tests", ["cargo", "test", "--locked", "--profile", "ci-test", "--bin",
                                      "systemless", "--features", "test-support", "gpu_resources::tests",
                                      "--", "--test-threads=1"], 1800):
            return 1
        if not run("probe-build", ["cargo", "build", "--locked", "--profile", "ci-test", "--bin",
                                   "systemless", "--features", "test-support"], 1800):
            return 1
        if not run("native-window", [sys.executable, str(root / ".github/scripts/verify-native-window.py"),
                                     "--output", str(output / "windows")],
                   native_matrix_timeout(root / ".github/scripts/verify-native-window.py", [])):
            return 1
        if args.soak_seconds:
            if not run("lifecycle-soak", [sys.executable, str(root / ".github/scripts/verify-native-window.py"),
                                          "--soak-only", "--soak-seconds", str(args.soak_seconds),
                                          "--runtime", args.soak_runtime, "--presenter", args.soak_presenter,
                                          "--output", str(output / "soak")], native_matrix_timeout(
                                              root / ".github/scripts/verify-native-window.py",
                                              ["--soak-only", "--soak-seconds", str(args.soak_seconds),
                                               "--runtime", args.soak_runtime, "--presenter", args.soak_presenter],
                                              args.soak_seconds)):
                return 1
    if args.benchmarks:
        for architecture in ("68k", "ppc"):
            if not run("benchmarks-" + architecture, [sys.executable, str(root / ".github/scripts/benchmark-renderers.py"),
                                      "--architecture", architecture,
                                      "--output", str(output / ("benchmarks-" + architecture))], 7200):
                return 1
    else:
        report["pending"].append("production release benchmarks for 68k and PPC")
    if args.compute_soak_seconds:
        for architecture in ("68k", "ppc"):
            if not run("compute-resources-" + architecture, [sys.executable, str(root / ".github/scripts/soak-gpu-compute.py"),
                                             "--architecture", architecture,
                                             "--seconds", str(args.compute_soak_seconds),
                                             "--output", str(output / ("compute-" + architecture))], args.compute_soak_seconds + 2700):
                return 1
    else:
        report["pending"].append("multi-hour active guest compute resource stress for 68k and PPC")
    report["automated_checks_passed"] = True
    adapters = set()
    for log in output.rglob("*.log"):
        for line in log.read_text(errors="replace").splitlines():
            if "[GPU-ADAPTER] " in line:
                adapter = json.loads(line.split("[GPU-ADAPTER] ", 1)[1])
                adapters.add(json.dumps(adapter, sort_keys=True))
    report["adapters"] = [json.loads(adapter) for adapter in sorted(adapters)]
    save()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
