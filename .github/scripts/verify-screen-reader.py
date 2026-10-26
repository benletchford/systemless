#!/usr/bin/env python3
"""Drive the egui workspace through the accessibility API screen readers use.

Linux uses AT-SPI, which Orca reads; run it inside a D-Bus session with the
accessibility bus enabled (see native-window.yml). Windows uses UI
Automation, which NVDA and Narrator read. The check launches the
source-built Toolbox Showcase, finds the workspace menus and the guest pane
by role and name, opens File > Game Options > Launch Modal Dialog…, then reads and operates the
projected Macintosh dialog: its checkbox state, editable text and buttons.
This verifies what a screen reader receives and can do; it is not a person
listening to one.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--binary", type=Path)
parser.add_argument("--output", type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parents[2]
binary = args.binary or root / "target/ci-test/systemless"
output = args.output or root / "native-window-results" / "screen-reader"
output.mkdir(parents=True, exist_ok=True)


class AtSpi:
    """AT-SPI 2 through GObject introspection."""

    def __init__(self, pid):
        import gi
        gi.require_version("Atspi", "2.0")
        from gi.repository import Atspi
        self.atspi = Atspi
        self.pid = pid

    def _application(self):
        desktop = self.atspi.get_desktop(0)
        for index in range(desktop.get_child_count()):
            app = desktop.get_child_at_index(index)
            try:
                if app is not None and app.get_process_id() == self.pid:
                    return app
            except Exception:
                continue
        return None

    def nodes(self):
        app = self._application()
        if app is None:
            return []
        found, stack = [], [app]
        while stack and len(found) < 5000:
            node = stack.pop()
            try:
                states = node.get_state_set()
                text = None
                if node.get_text_iface() is not None:
                    text = self.atspi.Text.get_text(node, 0, -1)
                found.append({
                    "role": node.get_role_name(),
                    "name": node.get_name() or "",
                    "checked": states.contains(self.atspi.StateType.CHECKED),
                    "enabled": states.contains(self.atspi.StateType.ENABLED),
                    "text": text,
                    "handle": node,
                })
                stack.extend(node.get_child_at_index(i) for i in reversed(range(node.get_child_count())))
            except Exception:
                continue
        return [n for n in found if n["handle"] is not None]

    def activate(self, node, expand=False):
        action = node["handle"].get_action_iface()
        if action is None or self.atspi.Action.get_n_actions(action) == 0:
            raise RuntimeError(f"{node['role']} {node['name']!r} offers no action")
        self.atspi.Action.do_action(action, 0)


UIA_SCRIPT = r"""
param([int]$Target, [string]$Invoke)
$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$A = [System.Windows.Automation.AutomationElement]
$condition = New-Object System.Windows.Automation.PropertyCondition($A::ProcessIdProperty, $Target)
$nodes = @()
foreach ($window in $A::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $condition)) {
  $nodes += $window
  $nodes += $window.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
}
function Pattern($node, $pattern) {
  $value = $null
  if ($node.TryGetCurrentPattern($pattern, [ref]$value)) { return $value }
  return $null
}
if ($Invoke) {
  $request = $Invoke | ConvertFrom-Json
  foreach ($node in $nodes) {
    $info = $node.Current
    if ($info.Name -eq $request.name -and $info.ControlType.ProgrammaticName -eq $request.role) {
      $toggle = Pattern $node ([System.Windows.Automation.TogglePattern]::Pattern)
      if ($toggle) { $toggle.Toggle(); Write-Output "Toggle"; exit 0 }
      $expand = Pattern $node ([System.Windows.Automation.ExpandCollapsePattern]::Pattern)
      if ($expand -and $request.expand) { $expand.Expand(); Write-Output "ExpandCollapse"; exit 0 }
      $invokePattern = Pattern $node ([System.Windows.Automation.InvokePattern]::Pattern)
      if ($invokePattern) { $invokePattern.Invoke(); Write-Output "Invoke"; exit 0 }
      if ($expand) { $expand.Expand(); Write-Output "ExpandCollapse"; exit 0 }
      Write-Error "no pattern"; exit 2
    }
  }
  Write-Error "not found"; exit 3
}
$result = foreach ($node in $nodes) {
  $info = $node.Current
  $toggle = Pattern $node ([System.Windows.Automation.TogglePattern]::Pattern)
  $value = Pattern $node ([System.Windows.Automation.ValuePattern]::Pattern)
  [pscustomobject]@{
    role = $info.ControlType.ProgrammaticName
    name = $info.Name
    checked = ($toggle -ne $null -and $toggle.Current.ToggleState -eq 'On')
    enabled = $info.IsEnabled
    text = $(if ($value) { $value.Current.Value } else { $null })
  }
}
ConvertTo-Json -InputObject @($result) -Depth 3 -Compress
"""


class UiAutomation:
    """Windows UI Automation through PowerShell's managed client."""

    def __init__(self, pid):
        self.pid = pid
        self.script = Path(tempfile.mkdtemp()) / "uia.ps1"
        self.script.write_text(UIA_SCRIPT, encoding="utf-8")

    def _run(self, *extra):
        return subprocess.run(
            ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(self.script),
             "-Target", str(self.pid), *extra],
            capture_output=True, text=True, encoding="utf-8", timeout=60)

    def nodes(self):
        result = self._run()
        if result.returncode != 0 or not result.stdout.strip():
            return []
        return json.loads(result.stdout)

    def activate(self, node, expand=False):
        request = json.dumps({"name": node["name"], "role": node["role"], "expand": expand})
        result = self._run("-Invoke", request)
        if result.returncode != 0:
            raise RuntimeError(f"{node['role']} {node['name']!r}: {result.stderr.strip()}")
        record(f"  {node['name']!r} activated through {result.stdout.strip()}")


# Platform role names for the roles this check relies on.
if sys.platform == "win32":
    ROLES = {
        "button": {"ControlType.Button", "ControlType.MenuItem"},
        "checkbox": {"ControlType.CheckBox"},
        "text": {"ControlType.Edit"},
        "image": {"ControlType.Image"},
    }
else:
    ROLES = {
        "button": {"push button", "menu item", "check menu item", "menu", "toggle button"},
        "checkbox": {"check box"},
        "text": {"text", "entry", "editbar"},
        "image": {"image"},
    }

log = []


def record(message):
    print(message, flush=True)
    log.append(message)


def wait_for(backend, role, name, timeout=60, gone=False):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        # Menu items carry their shortcut after the name ("Preferences… ⌘P").
        matches = [n for n in backend.nodes()
                   if n["role"] in ROLES[role]
                   and (n["name"] == name or n["name"].startswith(name + " ")
                        or (n.get("text") or "") == name)]
        if matches and not gone:
            return matches[0]
        if gone and not matches:
            return None
        time.sleep(0.5)
    snapshot = sorted({(n["role"], n["name"]) for n in backend.nodes()})
    (output / "tree.json").write_text(json.dumps(snapshot, indent=1))
    state = "still present" if gone else "not found"
    raise SystemExit(f"{role} {name!r} {state}; tree saved to {output / 'tree.json'}")


def open_menu(backend, title, item):
    """Open a menu title and return the item inside it. Some UI Automation
    clients expand menus rather than invoking them, so both are tried."""
    for expand in (False, True):
        backend.activate(wait_for(backend, "button", title), expand=expand)
        try:
            return wait_for(backend, "button", item, timeout=15)
        except SystemExit:
            record(f"  {item!r} did not appear after activating {title!r}")
    return wait_for(backend, "button", item)


def main():
    case = Path(tempfile.mkdtemp())
    archive = case / "toolbox-showcase.sit"
    shutil.copy2(root / "tests/toolbox-showcase/toolbox-showcase.sit", archive)
    env = dict(os.environ, SYSTEMLESS_DESKTOP_RUNTIME="thread", SYSTEMLESS_TRACE_ACCESSIBILITY="1")
    for name in ("SYSTEMLESS_NATIVE_WINDOW_PROBE", "SYSTEMLESS_GPU_FAULT"):
        env.pop(name, None)
    with (output / "run.log").open("w") as run_log:
        process = subprocess.Popen([str(binary), str(archive)], cwd=case, env=env,
                                   stdout=run_log, stderr=subprocess.STDOUT)
        try:
            backend = UiAutomation(process.pid) if sys.platform == "win32" else AtSpi(process.pid)
            # Workspace chrome and the guest's own menus are exposed by name.
            for name in ("Workspace", "Help", "Pages", "File"):
                wait_for(backend, "button", name, timeout=120)
                record(f"menu {name!r} exposed")
            screen = wait_for(backend, "image", "Macintosh screen")
            record(f"guest pane exposed as {screen['role']!r}")
            # Open a Systemless-owned dialog through a guest submenu.
            backend.activate(open_menu(backend, "File", "Game Options"))
            backend.activate(wait_for(backend, "button", "Launch Modal Dialog…"))
            record("File > Game Options > Launch Modal Dialog… activated")
            checkbox = wait_for(backend, "checkbox", "Enable 3D Hardware Acceleration")
            before = checkbox["checked"]
            record(f"checkbox exposed, checked={before}")
            pilot = wait_for(backend, "text", "Cade Connelly")
            record(f"editable text exposed as {pilot['role']!r} with its value")
            for name in ("OK", "Cancel"):
                wait_for(backend, "button", name)
            record("dialog buttons exposed")
            backend.activate(checkbox)
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                now = wait_for(backend, "checkbox", "Enable 3D Hardware Acceleration")
                if now["checked"] != before:
                    break
                time.sleep(0.5)
            else:
                raise SystemExit("toggling the checkbox through the accessibility API had no effect")
            record(f"checkbox toggled to checked={now['checked']}")
            backend.activate(wait_for(backend, "button", "Cancel"))
            wait_for(backend, "checkbox", "Enable 3D Hardware Acceleration", gone=True)
            record("Cancel dismissed the dialog")
            if process.poll() is not None:
                raise SystemExit(f"Systemless exited early with {process.returncode}")
        except SystemExit:
            run_log.flush()
            trace = [line for line in (output / "run.log").read_text(errors="replace").splitlines()
                     if line.startswith("[ACCESSIBILITY]")]
            print("accessibility requests received:", *trace[-20:], sep="\n  ", flush=True)
            raise
        finally:
            process.kill()
            process.wait()
            (output / "steps.txt").write_text("\n".join(log) + "\n")
    print("screen reader accessibility check: PASS")


main()
