# GPUI control font override adoption

Recognized standard buttons, checkboxes and radio buttons now pass supported
Appearance font/size/face overrides through the shared guest title recipe to
`ClassicLine::styled`. The painter uses original guest glyph resources,
advances and font metrics, with its existing smooth-outline/bitmap fallback.
Default component call sites retain their original API and font recipe.

Only flag bits 0x0187 (family, face, size, theme font and size delta) are newly
eligible. Other selected fields, popup overrides and unsupported control kinds
retain guest painting. Unselected fields do not change the recipe or eligibility.
Control identity, tracking, keyboard and accessibility action dispatch remain
on existing guest paths.

Validation: default production check passes (11.41 seconds). The existing
overlapping-control ownership regression now covers both supported font flags
and unsupported alignment; it passes (1 passed, 0 failed, 182 filtered out).
This is model/source evidence. Actual styled-control composed appearance,
glyph alignment, active/inactive state, clipping and input across CPU modes
and scales remain unqualified. Classic one-bit guest ink evidence is recorded
separately in `classic-control-font-overrides`. No complete font gate is claimed.
