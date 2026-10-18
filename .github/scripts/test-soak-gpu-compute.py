#!/usr/bin/env python3
"""Fail-closed compute resource evidence validation, without a GPU dependency."""
import importlib.util
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
