# Volume popup wheel ownership

Frontend Standard File wheel targeting now declines while a volume popup owns the panel. Worker ControlActivation independently checks current popup ownership before turning queued semantic actions into guest clicks. This prevents stale queued wheel/button actions from scrolling the underlying list or cancelling/selecting through unrelated coordinates.

The actual modal snapshot regression passes15.27s across its eight pointer/semantic cases in mono68k, colour68k, PPC8 and PPC16. It first proves a synthetic populated list is scrollable, then checks popup ownership removes the target. With an actual guest popup open, a worker ScrollDown attempt is rejected without changing pointer position or dismissing ownership. Existing actual Open-list wheel scrolling passes40.62s across four modes when no popup owns the panel.

This is shared helper/session and GPUI test-window evidence. It does not implement oversized volume-popup pointer autoscrolling or establish physical wheel delivery, all mode/scale compositions or production readiness. All broad release gates remain open.
