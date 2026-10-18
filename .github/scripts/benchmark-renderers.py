#!/usr/bin/env python3
"""Measure default/--gpu guest redraw and completed offscreen presentation.

Run on an otherwise idle host. This intentionally has no timing pass/fail
threshold: software adapters and shared CI machines do not establish physical
GPU performance. See RENDERING-PERFORMANCE.md for workload boundaries.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess


def main():
    repo = Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=15)
    parser.add_argument("--profile", choices=("release", "fast", "ci-test"), default="release")
    parser.add_argument("--runs", type=int, default=3)
    args = parser.parse_args()
    if args.samples < 3 or args.runs < 1:
        parser.error("use at least three samples and one run")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    environment = dict(os.environ, SYSTEMLESS_REQUIRE_GPU="1",
                       SYSTEMLESS_BENCH_SAMPLES=str(args.samples))
    records = []
    pattern = re.compile(r"\[RENDER-BENCH\] case=(\S+) backend=(\S+) n=(\d+) "
                         r"p50_us=([\d.]+) p95_us=([\d.]+)")
    command = ["cargo", "test", "--locked", "--profile", args.profile, "--bin",
               "systemless", "benchmark_", "--", "--ignored", "--nocapture",
               "--test-threads=1"]
    for run in range(args.runs):
        environment["SYSTEMLESS_BENCH_OUTPUT"] = str(output / f"captures-{run}")
        if run % 2:
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
        batch = [dict(run=run, case=m[0], backend=m[1], n=int(m[2]),
                      p50_us=float(m[3]), p95_us=float(m[4]))
                 for m in pattern.findall(text)]
        for name in ("ui", "drawing", "text", "3d", "sprites", "present-ui", "present-text"):
            if not any(record["case"] == name for record in batch):
                raise RuntimeError(f"missing {name} measurements in {log}")
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
    report = dict(profile=args.profile, source_sha256=source_digest.hexdigest(), host=platform.platform(), machine=platform.machine(),
                  revision=subprocess.check_output(["git", "rev-parse", "HEAD"],
                                                   cwd=repo, text=True).strip(),
                  dirty=subprocess.check_output(["git", "status", "--porcelain"],
                                                cwd=repo, text=True).splitlines(),
                  fixture_sha256=hashlib.sha256(fixture.read_bytes()).hexdigest(),
                  command=command, records=records)
    (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
