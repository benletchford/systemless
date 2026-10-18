#!/usr/bin/env python3
"""Ensure incomplete or corrupt timing evidence cannot qualify a renderer."""
import importlib.util
import json
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("benchmark_renderers", Path(__file__).with_name("benchmark-renderers.py"))
benchmark = importlib.util.module_from_spec(spec)
spec.loader.exec_module(benchmark)


def observations(samples=100):
    result = {(case, backend): list(range(samples)) for case in ("text",)
              for backend in ("default", "gpu")}
    result.update({(case, backend): list(range(30)) for case in benchmark.KERNEL_CASES
                   for backend in ("default", "gpu", "gpu-adaptive")})
    result.update({(case, "gpu"): list(range(300)) for case in ("present-ui", "present-text")})
    return result


def log_for(items):
    count = len(items.get(("text", "default"), []))
    lines = ["[RENDER-ORDER] " + json.dumps(dict(case="text", pairs=count, first_measured_backend="gpu"))]
    lines.append("[RENDER-DISPATCH] " + json.dumps(dict(case="text", requests=[[0, 0, 0, 0] for _ in range(count)])))
    for (case, backend), values in items.items():
        ordered = sorted(values)
        lines.append("[RENDER-SAMPLES] " + json.dumps(dict(case=case, backend=backend, samples_us=values)))
        lines.append(f"[RENDER-BENCH] case={case} backend={backend} n={len(values)} "
                     f"p50_us={ordered[len(values)//2]:.1f} "
                     f"p95_us={ordered[(len(values)*95+99)//100-1]:.1f}")
    return "\n".join(lines)


class EvidenceTests(unittest.TestCase):
    def parse(self, log, samples=100, system="Linux"):
        return benchmark.parse_batch(log, 2, samples, "text", system)

    def cpu_log(self, values=None, clock="CLOCK_THREAD_CPUTIME_ID"):
        values = list(range(100)) if values is None else values
        return log_for(observations()) + "\n" + "\n".join(
            "[RENDER-CPU-SAMPLES] " + json.dumps(dict(case="text", backend=backend, clock=clock, samples_us=values))
            for backend in ("default", "gpu"))

    def test_cpu_diagnostics_preserve_alignment_without_replacing_wall_times(self):
        values = [1000] + list(range(99))
        records = benchmark.parse_batch(self.cpu_log(values), 0, 100, "text", "Linux", thread_cpu=True)
        gpu = next(r for r in records if r['case']=='text' and r['backend']=='gpu')
        self.assertEqual(gpu['thread_cpu_samples_us'], values)
        self.assertEqual(gpu['thread_cpu_p99_us'], 98)
        self.assertEqual(gpu['samples_us'], list(range(100)))
        self.assertEqual(gpu['p99_us'], 98)
        self.assertTrue(all(r['thread_cpu_samples_us'] is None for r in records if r['case']!='text'))

    def test_windows_thread_clock_is_explicitly_identified(self):
        log=self.cpu_log(clock='GetThreadTimes(user+kernel)')
        records=benchmark.parse_batch(log, 0, 100, 'text', 'Windows', thread_cpu=True)
        row=next(r for r in records if r['case']=='text' and r['backend']=='gpu')
        self.assertEqual(row['thread_cpu_clock'], 'GetThreadTimes(user+kernel)')
        with self.assertRaisesRegex(ValueError, 'invalid thread CPU'):
            benchmark.parse_batch(log, 0, 100, 'text', 'Linux', thread_cpu=True)

    def test_cpu_diagnostics_are_explicit_and_complete(self):
        with self.assertRaisesRegex(ValueError, 'missing or unexpected thread CPU'):
            self.parse(self.cpu_log())
        with self.assertRaisesRegex(ValueError, 'missing or unexpected thread CPU'):
            benchmark.parse_batch(log_for(observations()), 0, 100, 'text', 'Linux', thread_cpu=True)
        with self.assertRaisesRegex(ValueError, 'missing or unexpected thread CPU'):
            benchmark.parse_batch(self.cpu_log().rsplit('\n',1)[0], 0, 100, 'text', 'Linux', thread_cpu=True)

    def test_cpu_diagnostics_reject_bad_clock_data_and_duplicates(self):
        for values in ([0]*99, [float('nan')]*100, [float('inf')]*100, [-1]*100, [True]*100):
            with self.subTest(values=values[:2]), self.assertRaisesRegex(ValueError, 'invalid thread CPU'):
                benchmark.parse_batch(self.cpu_log(values), 0, 100, 'text', 'Linux', thread_cpu=True)
        with self.assertRaisesRegex(ValueError, 'invalid thread CPU'):
            benchmark.parse_batch(self.cpu_log(clock='unknown'), 0, 100, 'text', 'Linux', thread_cpu=True)
        log=self.cpu_log()
        with self.assertRaisesRegex(ValueError, 'duplicate thread CPU'):
            benchmark.parse_batch(log+'\n'+log.splitlines()[-1], 0, 100, 'text', 'Linux', thread_cpu=True)

    def test_preserves_pair_observations_and_reports_p99_only_with_enough_samples(self):
        items = observations()
        items[("text", "gpu")] = [1000] + list(range(99))
        records = self.parse(log_for(items))
        gpu = next(r for r in records if r["case"] == "text" and r["backend"] == "gpu")
        self.assertEqual(gpu["samples_us"][0], 1000)
        self.assertEqual(gpu["p99_us"], 98)
        small = self.parse(log_for(observations(99)), 99)
        self.assertIsNone(next(r for r in small if r["case"] == "text")["p99_us"])

    def test_missing_backend_is_not_complete_evidence(self):
        items = observations()
        del items[("text", "gpu")]
        with self.assertRaisesRegex(ValueError, "missing or incomplete"):
            self.parse(log_for(items))

    def test_truncated_observations_are_rejected_even_when_summary_agrees(self):
        items = observations()
        items[("text", "gpu")].pop()
        with self.assertRaisesRegex(ValueError, "missing or incomplete"):
            self.parse(log_for(items))

    def test_raw_data_is_required_and_duplicate_records_are_rejected(self):
        log = log_for(observations())
        with self.assertRaisesRegex(ValueError, "raw observations"):
            self.parse("\n".join(line for line in log.splitlines() if not line.startswith("[RENDER-SAMPLES]")))
        with self.assertRaisesRegex(ValueError, "duplicate summary"):
            self.parse(log + "\n" + log)

    def test_nonfinite_negative_and_boolean_observations_are_rejected(self):
        log = log_for(observations())
        for invalid in (float("nan"), float("inf"), -1, True):
            altered = log.replace('"samples_us": [0,', '"samples_us": [' + json.dumps(invalid) + ',', 1)
            with self.subTest(invalid=invalid), self.assertRaisesRegex(ValueError, "invalid observations"):
                self.parse(altered)

    def test_logged_quantiles_must_match_raw_evidence(self):
        with self.assertRaisesRegex(ValueError, "summary does not match"):
            self.parse(log_for(observations()).replace("p50_us=50.0", "p50_us=500.0", 1))

    def test_missing_pair_order_is_rejected(self):
        log = log_for(observations())
        with self.assertRaisesRegex(ValueError, "missing guest pair order"):
            self.parse("\n".join(line for line in log.splitlines() if not line.startswith("[RENDER-ORDER]")))

    def test_dispatch_samples_preserve_timing_alignment_and_zero_work(self):
        log = log_for(observations())
        rows = [[0, 0, 0, 0] for _ in range(100)]
        rows[99] = [7, 2, 1, 3]
        log = "\n".join(line for line in log.splitlines() if not line.startswith("[RENDER-DISPATCH]"))
        log += "\n[RENDER-DISPATCH] " + json.dumps(dict(case="text", requests=rows))
        records = self.parse(log)
        gpu = next(r for r in records if r["case"] == "text" and r["backend"] == "gpu")
        self.assertEqual(gpu["gpu_requests_per_sample"], rows)
        self.assertEqual(gpu["samples_us"][99], 99)
        self.assertTrue(all(r["gpu_requests_per_sample"] is None for r in records if r is not gpu))

    def test_missing_duplicate_and_invalid_dispatch_are_rejected(self):
        log = log_for(observations())
        lines = log.splitlines()
        telemetry = next(line for line in lines if line.startswith("[RENDER-DISPATCH]"))
        without = "\n".join(line for line in lines if line != telemetry)
        with self.assertRaisesRegex(ValueError, "missing guest dispatch"):
            self.parse(without)
        with self.assertRaisesRegex(ValueError, "duplicate guest dispatch"):
            self.parse(log + "\n" + telemetry)
        for rows in ([], [[0]*4]*99, [[0]*3]*100, [[True, 0, 0, 0]]*100,
                     [[-1, 0, 0, 0]]*100, [[0, 1, 0, 0]]*100,
                     [[1, 0, 2, 0]]*100, [[1.5, 0, 0, 0]]*100):
            with self.subTest(rows=rows[:1]), self.assertRaisesRegex(ValueError, "invalid guest dispatch"):
                self.parse(without + "\n[RENDER-DISPATCH] " + json.dumps(dict(case="text", requests=rows)))

    def test_macos_requires_the_platform_comparison(self):
        with self.assertRaisesRegex(ValueError, "default-metal"):
            self.parse(log_for(observations()), system="Darwin")


if __name__ == "__main__":
    unittest.main()
