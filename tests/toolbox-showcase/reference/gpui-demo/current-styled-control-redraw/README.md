# Current styled control redraw captures

Production head `0edfd2a1`; executable/fixture SHA-256 in `provenance.json`.
The executable builds in 10.47s. Public rebuilt Toolbox Showcase receives actual
guest Option-F to apply Geneva18bold, then a title-area guest checkbox click.
Capture assertions verify preserved generation/bounds/font and toggled value.
All four composed frames were visually inspected: smooth standard labels and
checked value remain visible after redraw, while application-owned headings
remain guest bitmap ink. These selected cases exercise recovery after the
control-state freshness fix; they are not a complete font or state matrix.

Actual sidecars confirm mono68k scale0.75, colour68k scale1.5, PPC8 scale1 and
PPC16 scale1.5. PPC16 uses the capture helper's default when no screen-depth is
supplied; the CLI accepts only 1/2/4/8. Failed CLI attempts remain archived.
The restricted mono run completed guest assertions but failed before composed
rendering with a macOS UI-service error. All successful captures used service
access; the failed log is retained rather than counted as a pass.

Reproduce using the rebuilt public fixture at
`tests/toolbox-showcase/build/toolbox-showcase.sit`, current example executable,
`--capture-control-fonts-changed <output.png>` and `--capture-scale <scale>`.
Use `--screen-depth 1` or 8 for 68k; `--prefer-powerpc --screen-depth 8` for PPC8,
and `--prefer-powerpc` alone for PPC16. Verify actual depth in every sidecar.
Physical macOS input/observers, full font fidelity and native-reference parity
remain unqualified. No release gate closes from these selected images.
