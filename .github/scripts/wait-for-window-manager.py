#!/usr/bin/env python3
"""Wait for a live EWMH manager before testing native focus under Xvfb."""
import re
import subprocess
import time


def supporting_window(*arguments):
    try:
        result = subprocess.run(["xprop", *arguments, "_NET_SUPPORTING_WM_CHECK"],
                                capture_output=True, text=True, timeout=1)
    except subprocess.TimeoutExpired:
        return None
    match = re.search(r"window id # (0x[0-9a-fA-F]+)", result.stdout)
    return int(match[1], 16) if result.returncode == 0 and match else None


def wait_for_manager(timeout=30):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        window = supporting_window("-root")
        # Check the manager's own window too: a stale root property alone
        # does not establish that a manager is alive and handling windows.
        if window and supporting_window("-id", hex(window)) == window:
            print("Native qualification window manager is ready", flush=True)
            return
        time.sleep(0.05)
    raise SystemExit("Window manager did not become ready; native qualification cannot start")


if __name__ == "__main__":
    wait_for_manager()
