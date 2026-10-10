# Classic control font overrides

Classic standard buttons, checkboxes and radio buttons now use the shared
Appearance title recipe for family, size and face. Unknown CDEF generic-button
fallback retains its prior default. Styled text uses existing guest glyph
painting and clips nondefault ink to its label bounds. Dialog calls retain the
existing face0 label API and inactive palette behavior.

The actual classic drawing regression exercises all three control kinds on a
512x342 one-bit guest framebuffer: Geneva18 changes the ink, bold changes it
again, clearing overrides restores exact original pixels, and Geneva72 bold
preserves every pixel outside the control bounds. Bounds and control value
remain unchanged after each draw. Result: 1 passed, 0 failed, 6702 filtered
out; 0.14 seconds after a 32.13-second build. Final default application check
passes (31.11 seconds including Cargo lock wait).

The initial test fixture read an unsynchronized dispatcher port and therefore
compared empty captures. The final test obtains the actual guest port from
QuickDraw globals and asserts nonzero screen base and row bytes.

This is classic guest drawing evidence, not colour68k scene qualification,
GPUI override adoption, complete transfer-mode/alignment/color support, native
font fidelity, tracking or cross-CPU lifecycle qualification. Those remain open.
