# Destination inversion GPU probe

This standalone Metal probe establishes the blending operation needed by a
Systemless-owned cursor renderer extension. White source colour with the
OneMinusDestinationColor source factor, Zero destination factor and RGB-only
write mask produces exact byte inversion while preserving destination alpha.
The BGRA8Unorm target matches GPUI Apple's drawable format. Eight source pixels
cover black, white, primary colours, midpoints and transparent/partial alpha.
Each first inversion changes only the scissored pixel; each second inversion
restores both pixels exactly. Sixteen assertions pass on actual offscreen Metal.

Reproduce after building the GPUI example. Compile `probe.rs` directly with
Rust's `--extern metal=<resolved libmetal rlib>` and
`-L dependency=target/debug/deps`, then run the resulting executable with macOS
GPU service access. The exact observed command and hashes are in
`provenance.json`; dependency artifact filenames vary by build configuration.

This is a renderer prerequisite. It does not draw a guest cursor through GPUI,
qualify a complete scene, or close the cursor release gate. Integration must
append inversion after the complete GPUI scene, use the same live/headless
encoder and preserve atlas identity, clipping, hotspots, scale and all native
window input/accessibility/lifecycle callbacks. The current production cursor
continues to decline inverted pixels until that faithful path is established.
