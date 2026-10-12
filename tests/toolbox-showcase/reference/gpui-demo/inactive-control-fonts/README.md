# Inactive styled standard controls

The current shared frontend builds successfully (2m38s). The PPC16 scale1.5
capture applies Geneva18bold via actual guest Option-F, requests frontend
foreground loss, waits for the guest owner to become inactive and return to
its event loop, and verifies preserved control generation, bounds, value and
font override. The actual guest and composed images are retained. Visual review
confirms smooth standard labels, unchanged control state and inactive window
title presentation. Application-drawn text retains guest bitmap ink.

This is a simulated frontend request through real guest suspend handling and
the shared live/headless renderer. It does not establish physical macOS observer
integration, native font-reference parity or a complete scale/state matrix.
Other CPU capture requests remain pending. All release gates remain open.

The same executable completes mono68k scale0.75 and PPC8 scale1 suspension
captures with the same guest-state assertions. Both composed images were
reviewed: smooth labels retain their appearance and guest control state;
inactive title chrome is visible. Colour68k remains pending.

Colour68k scale1.5 also completes successfully with all preserved-state
assertions; its composed image was reviewed. The four selected modes now have
inactive styled button/checkbox evidence. Broader fonts, radio/group variants,
full scale matrix, physical activation and independent reference parity remain
open; this does not close a complete production release gate.
