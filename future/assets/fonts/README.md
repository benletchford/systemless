# System font substitute

NimbusSans-Regular.ttf is copied unchanged from the repository's licensed URW
asset at src/systems/macintosh/quickdraw/fonts/urw. Its source is
[twardoch/urw-core35-fonts](https://github.com/twardoch/urw-core35-fonts/tree/84e183c5fd17fba4846e6c2ee402bc2cc2f3e175),
commit 84e183c5fd17fba4846e6c2ee402bc2cc2f3e175 (version 2.00).
Copyright 2013, 2014, 2015 by (URW)++ Design & Development.

We choose the SIL Open Font License 1.1 option offered by upstream. LICENSE.OFL
and LICENSE.md are copied unchanged. The font retains its upstream name.

The independent runtime generates monochrome source strikes with ab_glyph,
using Mac Roman character mapping, rounded advances and a 0.5 coverage threshold.
Screen points at 72 dpi are pixels per em. Generated masks are editable guest
memory; destination pixel drawing remains the wgpu boundary. This is a substitute
for system/Chicago, Geneva/application and Helvetica selection (families 0, 3,
21), not Apple font software or a recreation of Apple's font designs. It currently
supports sizes 1–96 and at most 32 cached guest strikes per process.

The legacy implementation uses a different outline rasterizer and compatibility
advance tables, including a Coppet substitute at Geneva 9. Equal glyph pixels
or classic layout equivalence are not claimed by this first fallback slice.

Asset SHA-256: df5d1ce7b86c2e74974008623612bae136123a35ba7885de8a0721b68258edbc.
