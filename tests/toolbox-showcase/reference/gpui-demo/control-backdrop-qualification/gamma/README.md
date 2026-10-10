# Shared display-gamma correction

The preceding PPC8 shade counterexample came from resolving raw CLUT channels
without the actual display gamma. Both CPU paths now retain native indexed
backgrounds and resolve them with rgba_palette_from_clut_with_gamma, matching
the existing guest framebuffer conversion. Palette changes do not require a
guest redraw to recolour a retained background.

The current executable builds successfully. Four isolated paint-record tests
pass, including live palette changes with unchanged guest pixels. The actual
PPC8 scale1 Option-F preferences capture exits successfully; visual inspection
confirms the control backgrounds now match the surrounding grey panel without
replacement patches or filtering outlines. Original bounds and guest font
records remain asserted by the capture. The High Contrast control crossing
custom paint retains guest pixels when its backdrop is uncertain.

This is a selected current PPC8 raster checkpoint, not full CPU/scale/inactive,
physical interaction, performance or independent Macintosh qualification.
Previous source hashes and captures in the parent folder remain historical;
the hashes here pin this later correction. Production gates remain open.

The same executable also completes the colour 68k depth8 scale1.5 capture.
Visual inspection confirms matching panel backgrounds and original control
clipping; application-drawn headings retain guest bitmap ink. The current
no-default-features public library check completes successfully (41.51s).
These additional checks do not close the full release gates.
