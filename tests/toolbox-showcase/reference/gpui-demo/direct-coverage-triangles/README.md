# Direct coverage triangles

The smooth recognized-label painter now submits each existing coverage
rectangle as two solid GPUI path triangles instead of rebuilding a general
polygon and tessellating it on every paint. Original glyph masks, alpha,
guest advances, baseline and coordinates are unchanged.

Validation on 2026-10-11:

Source `src/bin/gpui_demo_text.rs` SHA-256:
`33543dcec39551662c56954afe3bf6dc8fbefe498c6f436058174b8abbd0522f`.
Capture executable SHA-256:
`765a000c957ac0b5562748c35af3449b9fb5358ba61a72f1a1e7bb8b37a560a2`.

- Default production application check: passed, 3.12 seconds.
- Shared compositor example build: passed, 9.80 seconds.
- Actual colour68k depth8 Pages menu capture at scale1.5: passed.
- The resulting PNG is byte-identical to
  `../current-pages-font-review/colour-scale1.5.png`, SHA-256
  `c7735ed20b30ecc6f25f9810244ac4eacc457997b4cfaf5691ae5bc91ee5b5fe`.

The change follows a stack sample of the pre-change full regression process
that observed smooth-label tessellation and scene insertion. It removes the
general tessellation operation; no measured live speedup is claimed. The
single capture establishes this scene's unchanged raster, not all fonts,
scales, CPUs, selection states or production performance. Broader interaction
regression and current-candidate qualification remain required.
