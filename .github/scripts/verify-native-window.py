#!/usr/bin/env python3
"""Run both runtime modes and presenters against an actual window server.

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
for backend, gpu in (("inline", False), ("thread", False), ("inline", True), ("thread", True)):
    name = backend + ("-wgpu" if gpu else "")
    case = output / name
    case.mkdir(exist_ok=True)
    archive = case / "toolbox-showcase.sit"
    shutil.copy2(root / "tests/toolbox-showcase/toolbox-showcase.sit", archive)
    report = case / "report.json"
    report.unlink(missing_ok=True)
    env = dict(os.environ, SYSTEMLESS_DESKTOP_RUNTIME=backend,
               SYSTEMLESS_NATIVE_WINDOW_PROBE=str(report))
    command = [str(root / "target/ci-test/systemless"), str(archive)]
    if gpu:
        command.append("--gpu")
    with (case / "run.log").open("w") as log:
        try:
            result = subprocess.run(
                command,
                cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT,
                timeout=120,
            )
            state = json.loads(report.read_text()) if report.exists() else {}
            expected_renderer = "wgpu" if gpu else "platform"
            passed = (result.returncode == 0 and state.get("passed") is True
                      and state.get("renderer") == expected_renderer)
        except subprocess.TimeoutExpired:
            passed = False
    print(f"{name}: {'PASS' if passed else 'FAIL'}", flush=True)
    if not passed:
        failed.append(name)
if failed:
    raise SystemExit("Native qualification failed: " + ", ".join(failed))
