# Direct coverage triangles

## Overlap correction

The initial unconditional optimization at `b9e1deeb` fails all four selected
styled-menu comparisons against an exact pre-change control: overlapping glyph
rectangles have even-odd fill cutouts that solid triangles do not preserve.
The painter now conservatively retains general tessellation whenever original
glyph ink boxes overlap. Disjoint glyphs still use direct triangles.

The corrected painter passes exact decoded RGBA comparisons against the
pre-optimization control for mono68k/0.75, colour68k/1.5, PPC8/2 and PPC16/1.
The control changes only the text source back to its `7ec422c2` snapshot;
production source is restored after building it. Source/executable provenance,
both failed initial and passing corrected comparisons, and build/check logs
are in `overlap-control`. Corrected production check passes in 4.38 seconds.

The document glyph-click/drag test passes all four modes and scales on the
initial optimization in 264.05 seconds. It predates this conservative overlap
correction; no corrected interaction pass is claimed. Both versions preserve
guest hit geometry. Broad regression and live performance remain open.

## Initial nonoverlapping scene evidence

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
