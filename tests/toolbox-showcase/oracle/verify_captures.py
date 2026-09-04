#!/usr/bin/env python3
"""Verify recorded capture identities, without claiming a fresh emulator run."""
import hashlib
import json
from pathlib import Path


def verify(path, expected):
    actual = hashlib.sha256(path.read_bytes()).hexdigest()
    if actual != expected:
        raise SystemExit(f"Stale capture evidence: {path.name}: {actual} != {expected}")


def main():
    fixture = Path(__file__).resolve().parents[1]
    manifests = sorted((fixture / "oracle").glob("*-capture.json"))
    if not manifests:
        raise SystemExit("No capture manifests found")
    for path in manifests:
        manifest = json.loads(path.read_text())
        verify(fixture / "toolbox-showcase.sit", manifest["fixture_sha256"])
        verify(fixture / manifest["scenario"], manifest["scenario_sha256"])
        count = 0
        for run in manifest["runs"]:
            for checkpoint in run["checkpoints"]:
                verify(fixture / checkpoint["file"], checkpoint["sha256"])
                count += 1
        print(f"{path.name}: fixture, scenario, and {count} capture hashes verified")


if __name__ == "__main__":
    main()
