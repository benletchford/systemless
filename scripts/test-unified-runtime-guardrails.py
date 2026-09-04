#!/usr/bin/env python3
"""Exercise the architecture checker against isolated, deliberately invalid trees."""

from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("check-unified-runtime-guardrails.sh")


class GuardrailTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="systemless-guardrail-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "scripts").mkdir()
        shutil.copyfile(SCRIPT, self.root / "scripts/check-unified-runtime-guardrails.sh")
        self.write("src/loader/ppc/mod.rs", """
pub struct PpcLoadedApp {
    cpu: Cpu,
}
pub struct PpcToolboxStartupState {
    menu_select_call: Call,
}
""")
        self.write("src/runner/mod.rs", """
pub struct FixtureRunner {
    cpu: Cpu,
    menu_hook_trampoline: u32,
}
""")
        self.write("src/trap/dispatch.rs", """
pub struct TrapDispatcher {
    menu_def_trampoline: u32,
}
""")
        self.write("src/process_context.rs", "fn attach_to() {}\n")
        self.write("src/execution_kernel.rs", "")
        self.write("src/guest_call.rs", "")
        self.baseline = "docs/architecture/unified-runtime-guardrail-baseline.txt"
        self.write(self.baseline, """
[ppc_loaded_app_fields]
cpu
[ppc_toolbox_startup_fields]
menu_select_call
[fixture_runner_fields]
cpu
menu_hook_trampoline
[trap_dispatcher_fields]
menu_def_trampoline
[fixture_runner_trampolines]
menu_hook_trampoline
[trap_dispatcher_trampolines]
menu_def_trampoline
fixture_runner_field_count=2
trap_dispatcher_field_count=1
ppc_import_action_call_native_occurrences=0
adapter_effect_conversions=0
current_task_rewrites=0
depth_context_queries=0
[compatibility_methods]
process_context.rs:attach_to
""")

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def replace(self, name, old, new):
        path = self.root / name
        text = path.read_text()
        self.assertIn(old, text)
        path.write_text(text.replace(old, new))

    def check(self, succeeds, diagnostic=None):
        result = subprocess.run(
            ["sh", str(self.root / "scripts/check-unified-runtime-guardrails.sh"), "--source-only"],
            capture_output=True, text=True, timeout=20,
        )
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode == 0, succeeds, output)
        if diagnostic:
            self.assertIn(diagnostic, output)

    def test_baseline_matches(self):
        self.check(True)

    def test_new_adapter_field_fails(self):
        self.replace("src/loader/ppc/mod.rs", "    cpu: Cpu,", "    cpu: Cpu,\n    menus: Menus,")
        self.check(False, "new entries")

    def test_removed_field_requires_baseline_reduction(self):
        self.replace("src/loader/ppc/mod.rs", "    cpu: Cpu,\n", "")
        self.check(False, "entries removed")
        self.replace(self.baseline, "[ppc_loaded_app_fields]\ncpu\n", "[ppc_loaded_app_fields]\n")
        self.check(True)

    def test_same_count_runner_field_replacement_fails(self):
        self.replace("src/runner/mod.rs", "    cpu: Cpu,", "    menu_state: Menus,")
        self.check(False, "new entries")

    def test_removed_runner_field_requires_count_and_name_reduction(self):
        self.replace("src/runner/mod.rs", "    cpu: Cpu,\n", "")
        self.replace(self.baseline, "[fixture_runner_fields]\ncpu\n", "[fixture_runner_fields]\n")
        self.check(False, "FixtureRunner top-level ownership shrank")
        self.replace(self.baseline, "fixture_runner_field_count=2", "fixture_runner_field_count=1")
        self.check(True)

    def test_direct_native_call_outside_edge_fails(self):
        self.write("src/manager.rs", "PpcImportAction::CallNative { entry: 0 }\n")
        self.check(False, "Direct native calls")

    def test_edge_conversion_is_not_a_manager_exception(self):
        self.write("src/guest_call.rs", "PpcImportAction::CallNative { entry: 0 }\neffect.into_ppc_import_action()\n")
        self.check(True)
        self.write("src/manager.rs", "effect.into_ppc_import_action()\n")
        self.check(False, "Adapter effect conversions")

    def test_removed_conversion_requires_baseline_reduction(self):
        self.replace(self.baseline, "adapter_effect_conversions=0", "adapter_effect_conversions=1")
        self.check(False, "Adapter effect conversions")

    def test_task_rewriting_fails(self):
        self.write("src/guest_call.rs", "effect.with_task(self.current_task())\n")
        self.check(False, "Current-task rewrites")

    def test_depth_inference_fails(self):
        self.write("src/manager.rs", "calls.suspended_m68k_context_depth()\n")
        self.check(False, "Depth-based context queries")

    def test_projection_rename_or_move_fails(self):
        self.write("src/process_context.rs", "fn attach_other() {}\n")
        self.check(False, "compatibility methods changed")
        self.write("src/process_context.rs", "")
        self.write("src/runner/mod.rs", (self.root / "src/runner/mod.rs").read_text() + "fn attach_to() {}\n")
        self.check(False, "compatibility methods changed")

    def test_removed_projection_requires_baseline_reduction(self):
        self.write("src/process_context.rs", "")
        self.check(False, "compatibility methods changed")
        self.replace(self.baseline, "process_context.rs:attach_to\n", "")
        self.check(True)

    def test_concrete_cpu_in_neutral_kernel_fails(self):
        self.write("src/execution_kernel.rs", "use ppc::PpcCpu;\n")
        self.check(False, "concrete CPU/import-action references")

    def test_missing_counter_fails_closed(self):
        self.replace(self.baseline, "adapter_effect_conversions=0\n", "")
        self.check(False, "missing or invalid adapter_effect_conversions")


if __name__ == "__main__":
    unittest.main()
