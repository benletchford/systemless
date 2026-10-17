# System font substitutes

NimbusSans-Regular.ttf and NimbusMonoPS-Regular.ttf are copied unchanged from
the repository's licensed URW assets at src/systems/macintosh/quickdraw/fonts/urw. Their source is
[twardoch/urw-core35-fonts](https://github.com/twardoch/urw-core35-fonts/tree/84e183c5fd17fba4846e6c2ee402bc2cc2f3e175),
commit 84e183c5fd17fba4846e6c2ee402bc2cc2f3e175 (version 2.00).
Copyright 2013, 2014, 2015 by (URW)++ Design & Development.

We choose the SIL Open Font License 1.1 option offered by upstream. LICENSE.OFL
and LICENSE.md are copied unchanged. The fonts retain their upstream names.

The independent runtime generates monochrome source strikes with ab_glyph,
using Mac Roman character mapping, rounded advances and a 0.5 coverage threshold.
Screen points at 72 dpi are pixels per em. Generated masks are editable guest
memory; destination pixel drawing remains the wgpu boundary. This is a substitute
for system/Chicago, Geneva/application and Helvetica selection (families 0, 3,
21). Nimbus Mono PS supplies fixed-width Monaco/Courier selection (families 4, 22).
These are not Apple font software or recreations of Apple's font designs. The
converter supports sizes 1–96 and at most 32 cached guest strikes per process, keyed by
bundled source and point size. Compatibility aliases share the same editable
guest handle; proportional and fixed-width sources cannot collide.

The legacy implementation uses a different outline rasterizer and compatibility
advance tables, including a Coppet substitute at Geneva 9. Equal glyph pixels
or classic layout equivalence are not claimed for these substitutes.

NimbusSans-Regular.ttf SHA-256: df5d1ce7b86c2e74974008623612bae136123a35ba7885de8a0721b68258edbc.

NimbusMonoPS-Regular.ttf SHA-256:
bf749d47bf69f4bf6963833afb66a3a5b6e62185bfffaf068b7a1b3f0276ba82.
Its glyph advances and maximum-width metric come from its own outlines/metadata;
no historical Monaco maximum-width override is included. At 9 points its normal
fixed advance rounds to 5 pixels; synthetic bold adds 1 pixel using the shared strike
operation. These are substitute metrics, not proof of classic Monaco equivalence.
