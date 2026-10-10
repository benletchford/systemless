# Retained marked composition checkpoint

Disjoint stages preserve original guest storage until lossless guarded commits.
The virtual UTF-16 model preserves independent untouched gaps; corrections can
revisit retained candidates or replace across candidates, retaining outer stage
fragments. Merged single-span commits preserve the corrected caret.

`state-tests.log`: 20 state tests pass, including the final merged commit fix.
`rapid-wheel-platform.log`: actual GPUI test-window integration passes in
mono 68k, colour 68k, PPC8 and PPC16, including successive wheel events before
repaint, geometry invalidation, clipped bounds and scrolled glyph hit-testing.
This platform binary predates the final merged single-span commit fix, which
is covered by the subsequent state log. `overflow-platform.log` is the earlier
menu-reserved viewport checkpoint. These are not physical IME tests.

Source hashes pin the archive's current source snapshot; they do not assert
that every earlier binary contained every later source change. No screenshot
or native Macintosh visual-oracle qualification is claimed by this archive.
Physical IME, broader modal/file overflow coverage and release gates remain open.

`final-worker.log`: the final-source production worker regression passes
(28.20 seconds), including retained transactions and preservation of styled
guest gaps in all four CPU/depth modes. The merged-range commit is covered
by state tests; it has no separate production-worker qualification yet.

The subsequent `merged-worker.log` closes the dedicated document worker gap:
merged candidate replacement passes all four modes (32.30 seconds), with
original owner guards, exact text/caret, suffix bytes and untouched suffix style
runs. `merged-worker-source-hashes.json` pins this later test checkpoint.
Modal/file merged transactions and physical IME remain unqualified.
