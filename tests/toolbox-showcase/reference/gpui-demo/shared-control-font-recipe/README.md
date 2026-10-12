# Shared control font recipe

Appearance control-title font resolution now lives in the architecture-neutral
Control Manager as `resolve_control_title_style`, exported with
`ControlTitleStyle` through the existing runner snapshot API. PPC guest drawing
delegates to it and adapts the resolved RGB words to its native color type.
Font families, meta/theme IDs, size deltas, face flags, static-text foreground
rules and signed-boundary handling are preserved from the original PPC path.

Validation: default production application check passes (15.59 seconds); the
existing PPC meta-font regression passes (1 passed, 0 failed, 6701 filtered
out), exercising known/unknown meta IDs, theme IDs and size flags through the
PPC adapter. Terminal logs and source hashes are retained here.

This is a prerequisite for faithful shared drawing. Classic 68k control labels
and GPUI ControlFontStyle overrides have not adopted the recipe yet. Mode,
justification and background paint are still unfinished. No cross-CPU control
scene, interaction or complete font fidelity qualification is claimed.
