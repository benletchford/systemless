# Source-outline monochrome conversion

macOS uses a statically linked FreeType 2.13.2 monochrome scan converter on
the same Skrifa-hinted outline used to establish the glyph bounds. This
component performs no font lookup or shaping and changes no advance, bearing,
baseline, font-resource precedence or layout decision. Guest FONT/NFNT strikes
bypass it. Smooth GPUI outline coverage keeps its existing path.
If context allocation or outline conversion fails, the same source outline's
existing thresholded coverage is retained with unchanged metrics. Conversion
failure does not authorize selecting a different font family.

The converter and headers come from the FreeType sources bundled with
freetype-sys 0.20.1. Original hashes are recorded in `freetype/source-manifest.json`.
The standalone context allocation is changed from a shared static object to
calloc/free so independent font generations do not share mutable state.
Scan conversion is otherwise unchanged. The adapter namespaces its symbols
and validates the borrowed contour arrays and output dimensions.

The selected upstream license is the FreeType License (FTL). The full license
and upstream alternative-license notice accompany the sources. No system or
Homebrew FreeType library is needed at runtime. Other platforms retain their
existing raster path; this component does not qualify them or establish
original Macintosh font fidelity.

The adapter uses default simple dropout control, excluding stubs. It does not
currently receive per-glyph TrueType SCANCTRL/SCANTYPE state from Skrifa; matching
this policy to all source-font instructions and historical Macintosh
rasterization remains a qualification requirement. The bundled Monaco
alias is an explicitly resolved Nimbus Mono source, not an original Apple bitmap
strike. Improved stroke continuity must not be described as proof of Apple font
fidelity.
