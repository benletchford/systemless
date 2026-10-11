# TextEdit completed-paint ownership

Canonical guest text, font/style, geometry, wrapping, alignment, selection,
activation and caret state are retained alongside completed native ink on both
CPU paths. Current queries must match that recipe as well as the original
surface pixels. Raw record/content changes no longer authorize premature GPUI
replacement. A completed native redraw establishes fresh ownership. List-cell
raster capture retains its existing separate ownership path.

Eight shared ownership/raster tests pass, including raw text, size, face,
justification, font, selection and line-table mutations at depths 1/8/16.
The actual GPUI plain-editor regression passes 12 CPU/depth/alignment cases
in 32.40s, including scale geometry, hit-testing and guest editing. This
establishes regression coverage, not independent Macintosh fidelity or live
physical-input qualification.

The first example test build failed because the preceding alignment-capture
change omitted two fields from a test-only Args initializer. Its failure log
is retained; the initializer defaults are corrected in this change.

The earlier long frontend suite remains a mixed-checkpoint run and cannot
certify this candidate. All production release gates remain open.

Styled halo editing passes all four CPU/depth modes (174.32s); spacing-style
editing also passes all four modes (56.85s). The combined log is retained.
