#!/usr/bin/env python3
"""Fail-closed compute resource evidence validation, without a GPU dependency."""
import importlib.util
import os
import subprocess
import sys
from pathlib import Path
import unittest
spec = importlib.util.spec_from_file_location("compute_soak", Path(__file__).with_name("soak-gpu-compute.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def evidence():
    result = []
    for cycle in range(2):
        item = dict(seconds=float(cycle), cycles=cycle, gpu_writebacks=[cycle + 1] * 4, complete=bool(cycle))
        for key, role in [("quickdraw", "QuickDraw compute"), ("qd3d", "QD3D compute")]:
            item[key] = dict(role=role, hal_counters_enabled=True, native_metal=None, allocator=None,
                             scratch=dict(buffers={"readback": 16}, capacity_bytes=16))
        result.append(item)
    return result


class EvidenceTests(unittest.TestCase):
    def test_memory_evidence_rejects_absent_or_failed_live_sampling(self):
        self.assertFalse(module.memory_evidence([])["complete"])
        good = dict(seconds=0, pid=os.getpid(), rss_kib=1)
        self.assertTrue(module.memory_evidence([good])["complete"])
        for bad in [None, True, -1, float("nan")]:
            self.assertFalse(module.memory_evidence([dict(rss_kib=bad)])["complete"])
        failure = dict(seconds=1, error="access denied", process_running=True)
        self.assertFalse(module.memory_evidence([good, failure])["complete"])
        failure["process_running"] = False
        self.assertTrue(module.memory_evidence([good, failure])["complete"])

    def test_real_child_process_memory_is_observed(self):
        # On Windows this validates the actual ABI, PID and permissions, rather
        # than pretending a mocked counter proves the collector works.
        child = subprocess.Popen([sys.executable, "-c", "import sys; x=bytearray(16*1024*1024); print('ready',flush=True); sys.stdin.read()"],
                                 stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        try:
            self.assertEqual(child.stdout.readline().strip(), "ready")
            row = module.process_memory(child.pid)
            self.assertGreater(row["rss_kib"], 0)
            if os.name == "nt":
                self.assertGreaterEqual(row["private_commit_bytes"], 16*1024*1024)
                self.assertGreaterEqual(row["peak_private_commit_bytes"], row["private_commit_bytes"])
                self.assertGreaterEqual(row["peak_working_set_bytes"], row["working_set_bytes"])
                self.assertEqual(row["rss_kib"], row["working_set_bytes"] // 1024)
        finally:
            child.communicate(timeout=10)

    @unittest.skipUnless(os.name == "nt", "Windows API failure path")
    def test_windows_invalid_pid_is_an_error(self):
        with self.assertRaises(OSError):
            module.process_memory(0)

    def test_complete_teardown_requires_all_owners_in_order(self):
        events = [f"{stage}:{event}" for stage in ("runners", "qd3d", "quickdraw", "report")
                  for event in ("begin", "end")]
        log = "\n".join("[GPU-SOAK-TEARDOWN] " + event for event in events)
        self.assertTrue(module.teardown_evidence(log)["complete"])
        for invalid in [events[:-1], events + events[-1:], list(reversed(events)), []]:
            text = "\n".join("[GPU-SOAK-TEARDOWN] " + event for event in invalid)
            self.assertFalse(module.teardown_evidence(text)["complete"])

    def test_stalled_teardown_preserves_last_entered_owner(self):
        result = module.teardown_evidence("unrelated log\n[GPU-SOAK-TEARDOWN] runners:begin\n")
        self.assertEqual(result["last_event"], "runners:begin")
        self.assertFalse(result["complete"])

    def test_complete_evidence_with_unknown_backend_memory_is_valid(self):
        self.assertTrue(module.validate_samples(evidence(), 1))

    def test_missing_or_wrong_compute_device_is_rejected(self):
        for key in ["quickdraw", "qd3d"]:
            for replacement in [None, {}, dict(role="presenter")]:
                data = evidence()
                data[-1][key] = replacement
                self.assertFalse(module.validate_samples(data, 1))

    def test_incomplete_or_short_runs_are_rejected(self):
        self.assertFalse(module.validate_samples(evidence()[:1], 1))
        self.assertFalse(module.validate_samples(evidence(), 2))
        data = evidence()
        data[-1]["complete"] = False
        self.assertFalse(module.validate_samples(data, 1))

    def test_nonfinite_negative_or_regressing_progress_is_rejected(self):
        for field, values in [("seconds", [float("nan"), float("inf"), -1, True]),
                              ("cycles", [-1, True])]:
            for value in values:
                data = evidence()
                data[-1][field] = value
                self.assertFalse(module.validate_samples(data, 1))
        data = evidence()
        data[0]["seconds"] = 2
        self.assertFalse(module.validate_samples(data, 1))

    def test_stopped_or_missing_gpu_writeback_is_rejected(self):
        for calls in [[1]*4, [2,2,0,2], [2]*3, [True]*4]:
            data = evidence()
            data[-1]["gpu_writebacks"] = calls
            self.assertFalse(module.validate_samples(data, 1))

    def test_corrupt_capacity_accounting_is_rejected(self):
        for scratch in [None, {}, dict(buffers={"x": -1}, capacity_bytes=-1),
                        dict(buffers={"x": 16}, capacity_bytes=32)]:
            data = evidence()
            data[-1]["quickdraw"]["scratch"] = scratch
            self.assertFalse(module.validate_samples(data, 1))


if __name__ == "__main__":
    unittest.main()
