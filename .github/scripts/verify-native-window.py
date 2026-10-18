#!/usr/bin/env python3
"""Run both runtime modes and presenters against an actual window server.

Linux needs a window manager (CI uses Xvfb + Openbox). macOS uses the hosted
runner's desktop. Local runs need an unlocked desktop for drawable/focus
qualification. No private archives or UI automation permissions are required. This is functional qualification, not a display-latency benchmark.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--recovery-only", action="store_true",
                    help="check renderer transitions without focus/fullscreen interaction")
parser.add_argument("--output", type=Path)
parser.add_argument("--soak-seconds", type=int, default=0)
parser.add_argument("--soak-only", action="store_true",
                    help="repeat platform/wgpu/software lifecycle checks in both runtime modes")
parser.add_argument("--runtime", choices=("all", "inline", "thread"), default="all")
parser.add_argument("--presenter", choices=("all", "platform", "wgpu", "software"), default="all")
parser.add_argument("--dry-run", action="store_true", help="validate duration and list cases without starting a desktop")
args = parser.parse_args()
if not 0 <= args.soak_seconds <= 21600:
    parser.error("--soak-seconds must be 0..21600")
if args.recovery_only and (args.soak_only or args.soak_seconds):
    parser.error("recovery-only cannot qualify lifecycle soak")
if (args.runtime != "all" or args.presenter != "all") and not args.soak_only:
    parser.error("case selectors require --soak-only; the normal matrix stays complete")
if args.soak_seconds > 3600 and (not args.soak_only or args.runtime == "all" or args.presenter == "all"):
    parser.error("longer-than-one-hour soaks require one explicit runtime and presenter")
root = Path(__file__).resolve().parents[2]
output = args.output or root / "native-window-results"
failed = []
cases = [] if args.recovery_only else [(backend, gpu, "", None) for backend in ("inline", "thread") for gpu in (False, True)]
cases += [] if args.soak_only else [(backend, True, fault, None) for backend in ("inline", "thread")
          for fault in ("startup", "loss", "fatal", "recreate", "repeated-loss")]
if not args.recovery_only and not args.soak_only:
    cases += [(backend, False, "", renderer) for backend in ("inline", "thread")
              for renderer in ("auto", "platform", "wgpu", "software")]
if args.soak_only:
    cases += [(backend, False, "", "software") for backend in ("inline", "thread")]
    cases = [case for case in cases
             if (args.runtime == "all" or case[0] == args.runtime)
             and (args.presenter == "all" or (case[3] or ("wgpu" if case[1] else "platform")) == args.presenter)]
if args.dry_run:
    print(json.dumps(cases))
    raise SystemExit(0)
output.mkdir(parents=True, exist_ok=True)
for backend, gpu, fault, renderer in cases:
    name = backend + ("-wgpu" if gpu else "")
    if renderer:
        name += "-renderer-" + renderer
    if fault:
        name += "-" + fault
    case = output / name
    case.mkdir(exist_ok=True)
    archive = case / "toolbox-showcase.sit"
    shutil.copy2(root / "tests/toolbox-showcase/toolbox-showcase.sit", archive)
    report = case / "report.json"
    report.unlink(missing_ok=True)
    env = dict(os.environ, SYSTEMLESS_DESKTOP_RUNTIME=backend,
               SYSTEMLESS_NATIVE_WINDOW_PROBE=str(report))
    env["SYSTEMLESS_GPU_FAULT"] = fault
    env["SYSTEMLESS_NATIVE_SOAK_SECONDS"] = str(args.soak_seconds)
    if args.recovery_only:
        env["SYSTEMLESS_NATIVE_RECOVERY_ONLY"] = "1"
    else:
        env.pop("SYSTEMLESS_NATIVE_RECOVERY_ONLY", None)
    command = [str(root / "target/ci-test/systemless"), str(archive)]
    if gpu:
        command.append("--gpu")
    if renderer:
        command.append("--renderer=" + renderer)
    with (case / "run.log").open("w") as log:
        try:
            process = subprocess.Popen(command, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
            started = time.monotonic()
            memory = []
            while process.poll() is None:
                if time.monotonic() - started > 120 + args.soak_seconds:
                    process.kill()
                    process.wait()
                    raise subprocess.TimeoutExpired(command, 120 + args.soak_seconds)
                if os.name == "posix":
                    sample = subprocess.run(["ps", "-o", "rss=", "-p", str(process.pid)],
                                            capture_output=True, text=True)
                    if sample.returncode == 0 and sample.stdout.strip().isdigit():
                        memory.append(dict(seconds=time.monotonic() - started,
                                           rss_kib=int(sample.stdout.strip())))
                time.sleep(1)
            (case / "memory.json").write_text(json.dumps(memory, indent=2) + "\n")
            state = json.loads(report.read_text()) if report.exists() else {}
            resources = state.get("gpu_resources", [])
            resource_contract = (
                len(resources) >= state.get("lifecycle_cycles", 0) + 1
                and resources[-1].get("renderer") == state.get("renderer")
                and all(isinstance(item.get("resources"), dict)
                        and item["resources"].get("hal_counters_enabled") is True
                        for item in resources if item.get("renderer") == "wgpu")
            )
            expected_renderer = "software" if renderer == "software" else ("wgpu" if (gpu or renderer == "wgpu") and fault in ("", "loss") else "platform")
            software_contract = renderer != "software" or (
                any(item.get("presentation_backend") == "software" for item in resources)
                and all(item.get("presentation_backend") in ("software", "none") for item in resources)
            )
            passed = (process.returncode == 0 and state.get("passed") is True
                      and resource_contract and software_contract
                      and state.get("renderer") == expected_renderer
                      and (args.recovery_only or state.get("lifecycle_cycles", 0) >= (2 if args.soak_seconds else 1))
                      and state.get("elapsed_seconds", 0) >= args.soak_seconds)
        except subprocess.TimeoutExpired:
            passed = False
    print(f"{name}: {'PASS' if passed else 'FAIL'}", flush=True)
    if not passed:
        failed.append(name)
if failed:
    raise SystemExit("Native qualification failed: " + ", ".join(failed))
