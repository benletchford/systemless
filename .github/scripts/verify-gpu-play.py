#!/usr/bin/env python3
"""Qualify opt-in GPU rendering through real guest applications.

Integer UI and sprite output must match CPU captures exactly. Native 3D uses
its script's component tolerances, since shader floating-point rounding can
vary between host backends. Logs prove that the opt-in backend was selected.
Strict device unit tests exercise every kernel, including operations which
adaptive dispatch keeps on CPU.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess


def main():
    repo = Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=repo / "target/debug" /
                        ("systemless.exe" if os.name == "nt" else "systemless"))
    parser.add_argument("--output", type=Path, default=repo / "gpu-play-results")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("build the GUI binary first: cargo build --locked --bin systemless")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    fixture = repo / "tests/toolbox-showcase/toolbox-showcase.sit"
    cases = (
        ("classic-ui", "--prefer-classic-68k", "toolbox-showcase.json", "68k", True,
         ("[GPU] QuickDraw adaptive dispatch enabled",)),
        ("native-3d", "--prefer-powerpc", "toolbox-showcase-ppc-qd3d.json", "ppc", False,
         ("[GPU] QD3D adaptive dispatch enabled",)),
        ("native-sprites", "--prefer-powerpc", "toolbox-showcase-ppc-sprites.json", "ppc", True,
         ("[GPU] QuickDraw adaptive dispatch enabled",)),
    )
    for name, architecture_flag, script, architecture, exact, markers in cases:
        reference = None
        for gpu in (False, True):
            label = name + ("-gpu" if gpu else "-cpu")
            case = output / label
            case.mkdir()
            archive = case / "toolbox-showcase.sit"
            shutil.copy2(fixture, archive)
            captures = case / "play"
            command = [str(binary), str(archive), "--headless", architecture_flag,
                       "--play-script", str(repo / "tests/play" / script),
                       "--play-output", str(captures)]
            if gpu:
                command.append("--gpu")
                if exact:
                    command.extend(("--play-reference", str(reference)))
            log_path = case / "run.log"
            with log_path.open("w", encoding="utf-8") as log:
                result = subprocess.run(command, cwd=repo, stdout=log,
                                        stderr=subprocess.STDOUT, timeout=180)
            log = log_path.read_text(encoding="utf-8", errors="replace")
            if result.returncode:
                raise RuntimeError(label + " failed:\n" + log[-12000:])
            report = json.loads((captures / "report.json").read_text(encoding="utf-8"))
            if (report.get("status") != "passed" or report.get("architecture") != architecture
                    or report.get("assertions", 0) == 0 or not report.get("checkpoints")):
                raise RuntimeError(label + " produced an incomplete play report")
            if gpu:
                missing = [marker for marker in markers if marker not in log]
                fallbacks = [line for line in log.splitlines()
                             if line.startswith("[GPU]") and "CPU fallback" in line]
                if missing or fallbacks:
                    raise RuntimeError(f"{label}: missing GPU markers {missing}; fallbacks {fallbacks}")
                if exact and any(point.get("differing_pixels") != 0
                                 for point in report["checkpoints"]):
                    raise RuntimeError(label + " did not match the CPU screenshots")
            else:
                if any(line.startswith("[GPU]") for line in log.splitlines()):
                    raise RuntimeError(label + " initialized GPU rendering without --gpu")
                reference = captures
            print(label + ": PASS", flush=True)


if __name__ == "__main__":
    main()
