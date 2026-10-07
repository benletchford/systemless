#!/usr/bin/env python3
"""Run both production runtime modes against an actual window server.

Linux needs a window manager (CI uses Xvfb + Openbox). macOS uses the hosted
runner's desktop. No private archives, UI permissions or unlocked local Mac are
required. This is functional qualification, not a display-latency benchmark.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parents[2]
output = root / "native-window-results"
output.mkdir(exist_ok=True)
failed = []
for backend in ("inline", "thread"):
    case = output / backend
    case.mkdir(exist_ok=True)
    archive = case / "toolbox-showcase.sit"
    shutil.copy2(root / "tests/toolbox-showcase/toolbox-showcase.sit", archive)
    report = case / "report.json"
    report.unlink(missing_ok=True)
    env = dict(os.environ, SYSTEMLESS_DESKTOP_RUNTIME=backend,
               SYSTEMLESS_NATIVE_WINDOW_PROBE=str(report))
    with (case / "run.log").open("w") as log:
        try:
            result = subprocess.run(
                [str(root / "target/ci-test/systemless"), str(archive)],
                cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT,
                timeout=120,
            )
            passed = result.returncode == 0 and report.exists() and json.loads(report.read_text())["passed"]
        except subprocess.TimeoutExpired:
            passed = False
    print(f"{backend}: {'PASS' if passed else 'FAIL'}", flush=True)
    if not passed:
        failed.append(backend)
if failed:
    raise SystemExit("Native qualification failed: " + ", ".join(failed))
