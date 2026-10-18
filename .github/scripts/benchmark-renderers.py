#!/usr/bin/env python3
"""Measure default/--gpu guest redraw and completed offscreen presentation.

Run on an otherwise idle host. This intentionally has no timing pass/fail
threshold: software adapters and shared CI machines do not establish physical
GPU performance. See RENDERING-PERFORMANCE.md for workload boundaries.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import subprocess


SUMMARY = re.compile(r"\[RENDER-BENCH\] case=(\S+) backend=(\S+) n=(\d+) "
                     r"p50_us=([\d.]+) p95_us=([\d.]+)")
GUEST_CASES = ("ui", "drawing", "text", "3d", "sprites")
KERNEL_CASES = ("3d-kernel-64x48", "3d-kernel-320x240", "3d-kernel-640x480",
                "3d-sparse-64", "3d-overdraw-8")


def parse_batch(log, run, samples, guest_case=None, system=None, thread_cpu=False):
    """Reject incomplete evidence; preserve chronological observations."""
    summaries = {}
    for case, backend, count, p50, p95 in SUMMARY.findall(log):
        key = (case, backend)
        if key in summaries:
            raise ValueError(f"duplicate summary: {key}")
        summaries[key] = (int(count), float(p50), float(p95))
    raw = {}
    orders = {}
    dispatch = {}
    cpu = {}
    cpu_clock = "GetThreadTimes(user+kernel)" if (system or platform.system()) == "Windows" else "CLOCK_THREAD_CPUTIME_ID"
    for line in log.splitlines():
        if "[RENDER-CPU-SAMPLES] " in line:
            item = json.loads(line.split("[RENDER-CPU-SAMPLES] ", 1)[1])
            key = (item["case"], item["backend"])
            values = item["samples_us"]
            if key in cpu:
                raise ValueError("duplicate thread CPU observations")
            if (item.get("clock") != cpu_clock or not isinstance(values, list)
                    or len(values) != samples
                    or any(isinstance(v, bool) or not isinstance(v, (int, float))
                           or not math.isfinite(v) or v < 0 for v in values)):
                raise ValueError("invalid thread CPU observations")
            cpu[key] = values
        if "[RENDER-DISPATCH] " in line:
            item = json.loads(line.split("[RENDER-DISPATCH] ", 1)[1])
            case = item["case"]
            if case in dispatch:
                raise ValueError("duplicate guest dispatch evidence")
            requests = item["requests"]
            if (not isinstance(requests, list) or len(requests) != samples
                    or any(not isinstance(row, list) or len(row) != 4
                           or any(type(value) is not int or value < 0 for value in row)
                           or row[1] > row[0] or row[2] > row[0] for row in requests)):
                raise ValueError("invalid guest dispatch evidence")
            dispatch[case] = requests
        if "[RENDER-ORDER] " in line:
            order = json.loads(line.split("[RENDER-ORDER] ", 1)[1])
            if order["case"] in orders:
                raise ValueError("duplicate guest pair order")
            if order["pairs"] != samples or order["first_measured_backend"] not in ("default", "gpu"):
                raise ValueError("invalid guest pair order")
            orders[order["case"]] = order["first_measured_backend"]
        if "[RENDER-SAMPLES] " not in line:
            continue
        item = json.loads(line.split("[RENDER-SAMPLES] ", 1)[1])
        key = (item["case"], item["backend"])
        values = item["samples_us"]
        if key in raw:
            raise ValueError(f"duplicate observations: {key}")
        if not values or any(isinstance(v, bool) or not isinstance(v, (int, float))
                             or not math.isfinite(v) or v < 0 for v in values):
            raise ValueError(f"invalid observations: {key}")
        raw[key] = values
    if raw.keys() != summaries.keys():
        raise ValueError("raw observations and summaries differ")
    guest_cases = (guest_case,) if guest_case else GUEST_CASES
    if set(orders) != set(guest_cases):
        raise ValueError("missing guest pair order")
    expected = {(case, backend): samples for case in guest_cases
                for backend in ("default", "gpu")}
    expected.update({(case, backend): 30 for case in KERNEL_CASES
                     for backend in ("default", "gpu", "gpu-adaptive")})
    expected.update({(case, "gpu"): 300 for case in ("present-ui", "present-text")})
    if (system or platform.system()) == "Darwin":
        expected.update({(case, "default-metal"): 300 for case in ("present-ui", "present-text")})
    for key, count in expected.items():
        if key not in raw or len(raw[key]) != count:
            raise ValueError(f"missing or incomplete observations: {key}; expected {count}")
    if set(dispatch) != set(guest_cases):
        raise ValueError("missing guest dispatch evidence")
    expected_cpu = {(case, backend) for case in guest_cases for backend in ("default", "gpu")} if thread_cpu else set()
    if set(cpu) != expected_cpu:
        raise ValueError("missing or unexpected thread CPU observations")
    records = []
    for (case, backend), values in raw.items():
        count, logged_p50, logged_p95 = summaries[(case, backend)]
        ordered = sorted(values)
        if count != len(values):
            raise ValueError(f"sample count mismatch: {(case, backend)}")
        p50 = ordered[count // 2]
        p95 = ordered[(count * 95 + 99) // 100 - 1]
        if abs(p50 - logged_p50) > 0.051 or abs(p95 - logged_p95) > 0.051:
            raise ValueError(f"summary does not match observations: {(case, backend)}")
        cpu_values = cpu.get((case, backend))
        cpu_ordered = sorted(cpu_values) if cpu_values is not None else None
        records.append(dict(run=run, case=case, backend=backend, n=count,
                            first_measured_backend=orders.get(case),
                            p50_us=p50, p95_us=p95,
                            p99_us=ordered[(count * 99 + 99) // 100 - 1] if count >= 100 else None,
                            samples_us=values,
                            thread_cpu_clock=cpu_clock if cpu_values is not None else None,
                            thread_cpu_samples_us=cpu_values,
                            thread_cpu_p50_us=cpu_ordered[count // 2] if cpu_ordered is not None else None,
                            thread_cpu_p95_us=cpu_ordered[(count * 95 + 99) // 100 - 1] if cpu_ordered is not None else None,
                            thread_cpu_p99_us=cpu_ordered[(count * 99 + 99) // 100 - 1] if cpu_ordered is not None and count >= 100 else None,
                            gpu_requests_per_sample=dispatch[case] if case in dispatch and backend == "gpu" else None))
    return records


def main():
    repo = Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=15)
    parser.add_argument("--profile", choices=("release", "fast", "ci-test"), default="release")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--guest-case", choices=("ui", "drawing", "text", "3d", "sprites"))
    parser.add_argument("--reverse-first", action="store_true")
    parser.add_argument("--thread-cpu", action="store_true", help="supplement guest wall timings with current-thread CPU accounting")
    args = parser.parse_args()
    if args.samples < 3 or args.runs < 1:
        parser.error("use at least three samples and one run")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    environment = dict(os.environ, SYSTEMLESS_REQUIRE_GPU="1",
                       SYSTEMLESS_BENCH_SAMPLES=str(args.samples))
    if args.thread_cpu:
        environment["SYSTEMLESS_BENCH_THREAD_CPU"] = "1"
    else:
        environment.pop("SYSTEMLESS_BENCH_THREAD_CPU", None)
    if args.guest_case:
        environment["SYSTEMLESS_BENCH_GUEST_CASE"] = args.guest_case
    else:
        environment.pop("SYSTEMLESS_BENCH_GUEST_CASE", None)
    records = []
    command = ["cargo", "test", "--locked", "--profile", args.profile, "--bin",
               "systemless", "benchmark_", "--", "--ignored", "--nocapture",
               "--test-threads=1"]
    for run in range(args.runs):
        environment["SYSTEMLESS_BENCH_OUTPUT"] = str(output / f"captures-{run}")
        if (run + int(args.reverse_first)) % 2:
            environment["SYSTEMLESS_BENCH_REVERSE"] = "1"
        else:
            environment.pop("SYSTEMLESS_BENCH_REVERSE", None)
        log = output / f"run-{run}.log"
        with log.open("w") as stream:
            result = subprocess.run(command, cwd=repo, env=environment,
                                    stdout=stream, stderr=subprocess.STDOUT, timeout=1800)
        text = log.read_text(errors="replace")
        if result.returncode:
            raise RuntimeError(f"benchmark failed; inspect {log}\n{text[-5000:]}")
        batch = parse_batch(text, run, args.samples, args.guest_case, thread_cpu=args.thread_cpu)
        records.extend(batch)
        for record in batch:
            print(f"run={run} {record['case']} {record['backend']}: "
                  f"p50={record['p50_us']:.1f}us p95={record['p95_us']:.1f}us",
                  flush=True)
    fixture = repo / "tests/toolbox-showcase/toolbox-showcase.sit"
    source_digest = hashlib.sha256()
    sources = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z",
         "--", "*.rs", "*.wgsl", "*.metal"], cwd=repo, text=True).split("\0")
    sources = [name for name in sources if name]
    for name in sorted(sources + ["Cargo.toml", "Cargo.lock"]):
        source_digest.update(name.encode() + b"\0" + (repo / name).read_bytes() + b"\0")
    report = dict(schema_version=4 if args.thread_cpu else 3,
                  thread_cpu_enabled=args.thread_cpu,
                  thread_cpu_scope="calling guest thread user+kernel CPU; encloses wall timer with boundary overhead; excludes GPU work and other worker threads; clock accounting granularity varies" if args.thread_cpu else None,
                  gpu_request_columns=["quickdraw", "glyph_masks", "deep_masks", "3d"],
                  guest_sample_order="alternating-per-iteration",
                  harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  p99_minimum_samples=100, profile=args.profile, guest_case=args.guest_case, reverse_first=args.reverse_first, source_sha256=source_digest.hexdigest(), host=platform.platform(), machine=platform.machine(),
                  revision=subprocess.check_output(["git", "rev-parse", "HEAD"],
                                                   cwd=repo, text=True).strip(),
                  dirty=subprocess.check_output(["git", "status", "--porcelain"],
                                                cwd=repo, text=True).splitlines(),
                  fixture_sha256=hashlib.sha256(fixture.read_bytes()).hexdigest(),
                  command=command, records=records)
    (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
