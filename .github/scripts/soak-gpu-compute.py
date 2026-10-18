#!/usr/bin/env python3
"""Exercise sustained real guest GPU writeback; collect resources, never approve rollout."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import subprocess
import time

TEST = "renderer_bench::forced_gpu_guest::qualify_gpu_compute_resources"


def process_memory(pid):
    """Observe the test PID; Windows private commit is not GPU residency."""
    if os.name == "nt":
        import ctypes
        from ctypes import wintypes

        class Counters(ctypes.Structure):
            # PROCESS_MEMORY_COUNTERS_EX: DWORDs followed by pointer-sized SIZE_Ts.
            _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD)] + [
                (name, ctypes.c_size_t) for name in (
                    "PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage",
                    "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage", "QuotaNonPagedPoolUsage",
                    "PagefileUsage", "PeakPagefileUsage", "PrivateUsage")]

        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        psapi = ctypes.WinDLL("psapi", use_last_error=True)
        kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        kernel.OpenProcess.restype = wintypes.HANDLE
        kernel.CloseHandle.argtypes = [wintypes.HANDLE]
        kernel.CloseHandle.restype = wintypes.BOOL
        psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD]
        psapi.GetProcessMemoryInfo.restype = wintypes.BOOL
        handle = kernel.OpenProcess(0x1000, False, pid)  # PROCESS_QUERY_LIMITED_INFORMATION
        if not handle:
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            counters = Counters()
            counters.cb = ctypes.sizeof(counters)
            if not psapi.GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb):
                raise ctypes.WinError(ctypes.get_last_error())
            return dict(rss_kib=counters.WorkingSetSize // 1024,
                        working_set_bytes=counters.WorkingSetSize,
                        peak_working_set_bytes=counters.PeakWorkingSetSize,
                        private_commit_bytes=counters.PrivateUsage,
                        peak_private_commit_bytes=counters.PeakPagefileUsage)
        finally:
            if not kernel.CloseHandle(handle):
                raise ctypes.WinError(ctypes.get_last_error())
    if os.name == "posix":
        sample = subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True)
        if sample.returncode == 0 and sample.stdout.strip().isdigit():
            return dict(rss_kib=int(sample.stdout.strip()))
        raise OSError(f"cannot read process RSS: {sample.stderr.strip()}")
    raise OSError(f"process memory sampling unsupported on {os.name}")


def memory_evidence(samples):
    valid = [row for row in samples if type(row.get("rss_kib")) is int and row["rss_kib"] >= 0
             and not row.get("error")]
    malformed = sum(not row.get("error") for row in samples) - len(valid)
    active_errors = [row for row in samples if row.get("error") and row.get("process_running")]
    return dict(samples=len(valid), active_errors=len(active_errors),
                malformed_samples=malformed, complete=bool(valid) and not active_errors and not malformed)


def teardown_evidence(log):
    prefix = "[GPU-SOAK-TEARDOWN] "
    events = [line.split(prefix, 1)[1].strip() for line in log.splitlines() if prefix in line]
    expected = [f"{stage}:{event}" for stage in ("runners", "qd3d", "quickdraw", "report")
                for event in ("begin", "end")]
    return dict(events=events, last_event=events[-1] if events else None, complete=events == expected)


def validate_samples(samples, seconds, workload="gpu"):
    if workload not in ("gpu", "cpu-baseline"):
        return False
    cpu_only = workload == "cpu-baseline"
    if not isinstance(samples, list) or len(samples) < 2:
        return False
    if not isinstance(samples[-1], dict):
        return False
    previous_time = -1
    previous_cycle = -1
    previous_calls = [0] * 4
    for index, sample in enumerate(samples):
        if not isinstance(sample, dict) or sample.get("complete") is not (index == len(samples) - 1):
            return False
        if sample.get("workload", "gpu") != workload:
            return False
        elapsed, cycle, calls = sample.get("seconds"), sample.get("cycles"), sample.get("gpu_writebacks")
        if (isinstance(elapsed, bool) or not isinstance(elapsed, (int, float)) or not math.isfinite(elapsed)
                or elapsed < 0 or elapsed < previous_time or type(cycle) is not int or cycle < 0 or cycle < previous_cycle
                or not isinstance(calls, list) or len(calls) != 4
                or any(type(v) is not int or (v != 0 if cpu_only else v <= 0 or v < old)
                       for v, old in zip(calls, previous_calls))):
            return False
        for key, role in [("quickdraw", "QuickDraw compute"), ("qd3d", "QD3D compute")]:
            data = sample.get(key, {})
            if cpu_only:
                if data is not None:
                    return False
                continue
            if not isinstance(data, dict):
                return False
            scratch = data.get("scratch", {})
            if not isinstance(scratch, dict):
                return False
            capacities = scratch.get("buffers", {})
            if (data.get("role") != role or data.get("hal_counters_enabled") is not True
                    or not isinstance(capacities, dict) or not capacities
                    or any(type(n) is not int or n <= 0 for n in capacities.values())
                    or type(scratch.get("capacity_bytes")) is not int
                    or scratch["capacity_bytes"] != sum(capacities.values())):
                return False
        previous_time, previous_cycle, previous_calls = elapsed, cycle, calls
    return (samples[-1]["seconds"] >= seconds and samples[-1]["cycles"] > 0
            and (cpu_only or all(last > first for last, first in zip(samples[-1]["gpu_writebacks"], samples[0]["gpu_writebacks"]))))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--seconds", type=int, default=7200)
    parser.add_argument("--workload", choices=("gpu", "cpu-baseline"), default="gpu")
    parser.add_argument("--profile", choices=("ci-test", "release"), default="ci-test")
    args = parser.parse_args()
    if not 1 <= args.seconds <= 21600:
        parser.error("--seconds must be 1..21600")
    root = Path(__file__).resolve().parents[2]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    def git(*parts):
        return subprocess.check_output(["git", *parts], cwd=root, text=True).strip()
    revision = git("rev-parse", "HEAD")
    sources = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z", "--",
         "*.rs", "*.wgsl", "*.metal", "Cargo.toml", "Cargo.lock"], cwd=root, text=True).split("\0")
    def source_digest():
        digest = hashlib.sha256()
        for name in sorted(n for n in sources if n):
            digest.update(name.encode() + b"\0" + (root / name).read_bytes() + b"\0")
        return digest.hexdigest()
    initial_digest = source_digest()
    report = dict(schema_version=4, revision=revision, dirty=git("status", "--porcelain").splitlines(),
                  source_sha256=initial_digest, harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  fixture_sha256=hashlib.sha256((root / "tests/toolbox-showcase/toolbox-showcase.sit").read_bytes()).hexdigest(),
                  host=platform.platform(), machine=platform.machine(), seconds=args.seconds, profile=args.profile, workload=args.workload,
                  gpu_qualification=args.workload == "gpu",
                  gpu_writeback_categories=["quickdraw_total", "masked_raster", "deep_mask_blit", "qd3d"],
                  gpu_writeback_scope="masked/deep categories are subsets of the QuickDraw total; do not sum",
                  status="building", default_eligible=False,
                  scope=("CPU guest allocation baseline only; never GPU qualification" if args.workload == "cpu-baseline" else
                         "forced guest GPU writeback/resource stress; not adaptive dispatch or performance qualification"),
                  rss_scope="test-process working set" if os.name == "nt" else "test-process RSS",
                  memory_scope="actual test PID only; Windows private commit and peaks are process-wide, not GPU residency; do not sum with HAL counters")
    def save():
        (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    save()
    children = []
    def collect():
        command = ["cargo", "test", "--locked", "--profile", args.profile, "--bin", "systemless",
                   "--features", "test-support", "--no-run", "--message-format=json"]
        report["build_command"] = command
        with (output / "build.jsonl").open("w") as messages, (output / "build.log").open("w") as log:
            result = subprocess.run(command, cwd=root, stdout=messages, stderr=log, timeout=1800)
        if result.returncode:
            report.update(status="failed", error="build failed")
            save()
            return 1
        executables = set()
        for line in (output / "build.jsonl").read_text().splitlines():
            item = json.loads(line)
            if (item.get("reason") == "compiler-artifact" and item.get("target", {}).get("name") == "systemless"
                    and item.get("profile", {}).get("test") and item.get("executable")):
                executables.add(item["executable"])
        if len(executables) != 1:
            raise RuntimeError("cannot identify the exact compute qualification executable")
        executable = Path(executables.pop())
        listing = subprocess.check_output([str(executable), "--ignored", "--list", "--exact", TEST], text=True)
        if TEST + ": test" not in listing:
            raise RuntimeError("compiled executable is missing compute qualification")
        if source_digest() != initial_digest or revision != git("rev-parse", "HEAD"):
            raise RuntimeError("source changed during build")
        report["executable"] = str(executable)
        report["binary_sha256"] = hashlib.sha256(executable.read_bytes()).hexdigest()
        command = [str(executable), "--ignored", "--exact", TEST, "--nocapture", "--test-threads=1"]
        report.update(status="running", command=command)
        save()
        env = dict(os.environ, SYSTEMLESS_REQUIRE_GPU="1", SYSTEMLESS_GPU_COMPUTE_SOAK_SECONDS=str(args.seconds),
                   SYSTEMLESS_GPU_COMPUTE_SOAK_REPORT=str(output / "resources.jsonl"))
        env.pop("SYSTEMLESS_GPU_COMPUTE_SOAK_CPU_ONLY", None)
        if args.workload == "cpu-baseline":
            env.pop("SYSTEMLESS_REQUIRE_GPU", None)
            env["SYSTEMLESS_GPU_COMPUTE_SOAK_CPU_ONLY"] = "1"
        memory = []
        started = time.monotonic()
        with (output / "run.log").open("w") as log, (output / "memory.jsonl").open("w") as memory_log:
            process = subprocess.Popen(command, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
            children.append(process)
            while process.poll() is None:
                if time.monotonic() - started > args.seconds + 600:
                    process.kill()
                    process.wait()
                    report["error"] = "compute qualification timed out"
                    break
                observation = dict(seconds=time.monotonic() - started, pid=process.pid)
                try:
                    observation.update(process_memory(process.pid))
                except OSError as error:
                    # Preserve an exit race, but never hide missing live-process evidence.
                    observation.update(error=str(error), process_running=process.poll() is None)
                memory.append(observation)
                memory_log.write(json.dumps(observation) + "\n")
                memory_log.flush()
                time.sleep(1)
        (output / "memory.json").write_text(json.dumps(memory, indent=2) + "\n")
        samples = [json.loads(line) for line in (output / "resources.jsonl").read_text().splitlines()] if (output / "resources.jsonl").exists() else []
        workload_complete = validate_samples(samples, args.seconds, args.workload)
        teardown = teardown_evidence((output / "run.log").read_text(errors="replace"))
        memory_report = memory_evidence(memory)
        passed = (process.returncode == 0 and workload_complete and teardown["complete"] and memory_report["complete"]
                  and revision == git("rev-parse", "HEAD") and source_digest() == initial_digest
                  and hashlib.sha256((root / "tests/toolbox-showcase/toolbox-showcase.sit").read_bytes()).hexdigest() == report["fixture_sha256"])
        report.update(status=("diagnostic_passed" if args.workload == "cpu-baseline" else "passed") if passed else "failed", returncode=process.returncode,
                      elapsed_seconds=time.monotonic() - started, resource_samples=len(samples),
                      workload_complete=workload_complete, teardown=teardown, memory=memory_report)
        save()
        print(f"compute resource soak: {report['status']} ({len(samples)} observations)", flush=True)
        return 0 if passed else 1
    try:
        return collect()
    except (Exception, KeyboardInterrupt) as error:
        report.update(status="failed", error=f"{type(error).__name__}: {error}")
        print(f"compute resource soak failed: {error}", flush=True)
        return 1
    finally:
        for process in children:
            if process.poll() is None:
                process.kill()
                process.wait()
        save()


if __name__ == "__main__":
    raise SystemExit(main())
