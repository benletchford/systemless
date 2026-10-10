# Macintosh typography in GPUI

GPUI is the desktop presentation frontend. The guest Font Manager, TextEdit,
Menu Manager and Toolbox remain authoritative for font resolution, layout and
editing. Systemless smooths supported resolved outlines at display resolution
without measuring the text again with a host font.

This policy applies to recognized standard menus, window titles, controls,
dialogs, lists, Standard File panels and TextEdit fields. Application-drawn
text retains guest rendering until its ownership and faithful replacement are
established. Current implementation and qualification limits are recorded in
[GPUI_COVERAGE.md](GPUI_COVERAGE.md).

## Font identity and fidelity

The smooth glyph API receives the exact glyph and source bytes already resolved
for guest drawing. It uses the outline registered for that source. Authentic
guest resources and explicit font overrides keep their existing precedence.
The presentation path does not select a new host family, weight or italic face.

Systemless's bundled fallback outlines can differ from original Apple bitmap
strikes. Matching the current guest renderer proves consistency with that
resolved source; it does not prove historical Macintosh font fidelity. A claim
of original font fidelity requires evidence using the actual guest font
resources and an independent native reference.

| Resolved source or paint policy | Current presentation policy |
| --- | --- |
| Supported outline glyphs | Antialiased coverage from the original resolved outline at raster scales 1–8. |
| Bitmap-only FONT/NFNT glyphs | Preserve the resolved guest glyph pixels; the smooth resolver declines them. No outline or host font is silently substituted. |
| Missing glyph or unsupported source/style | Retain the complete qualified binary recipe or guest-owned source region. Whole-run and whole-field resolution prevents partial text replacement. |
| Unsupported scaled strikes, justification, custom backgrounds or definitions | Keep guest paint until the complete geometry, colour and ownership policy is supported and tested. |
| Application-owned text | Preserve guest paint until faithful replacement is established. |

Bitmap-only fonts retain their pixel structure. Blurring those pixels cannot
create the missing outline. Any future bitmap-to-outline conversion needs an
explicit policy and font, bounds and interaction qualification.

## Layout, colour and interaction

Guest insertion positions and native glyph paint pens have distinct roles.
GPUI preserves both: insertion positions drive pointer mapping, selections and
carets; paint pens locate glyph ink. Menu negative-bearing normalization can
translate paint pens without changing insertion advances.

Guest bounds, baselines, wrapping, alignment, clipping and scrolling remain
authoritative. Display-resolution coverage changes ink samples within the
existing presentation transform. Pointer and keyboard operations continue
through the guest event and Toolbox paths.

Styled TextEdit resolves every operation before choosing smooth presentation.
It retains the CPU-specific line/selection paint order, clips to the guest view,
and paints the caret last. Normal and inverted physical colour pairs are kept
separately because Macintosh indexed-colour inversion can differ from an RGB
complement. Unsupported operations decline the complete field.

## Synthetic styles

Basic bold uses the native one-guest-pixel rightward synthesis. Underlines keep
the guest CPU's policy: classic TextEdit has per-character descender gaps,
PPC TextEdit uses its continuous run ribbon, and ordinary menu labels keep their
continuous line underline. Guest advances and paint advances remain unchanged.

Italic presentation uses the original upright outline and the native
baseline/descent pivot. Raster 1 keeps the native integer row shear. Higher
rasters use its continuous half-pixel slope, distribute half-device-pixel
coverage between neighbours, and clamp shifts to the same native envelope.
This explicitly improves display-resolution edges while preserving source
identity, baseline, height, advances and clipping. Higher-raster ink samples
are not claimed to reproduce the original bitmap pixels.

Non-underlined supported outline/shadow styles use the native smear/exclusion
policy. Condensed/extended styles preserve the current guest recipe's spacing
adjustment without stretching the resolved glyph. Native paint pens and guest
insertion positions remain separate; PPC's minimum advance remains intact.
Both flags together cancel their spacing adjustments. The original outline
coverage and its bounds remain unchanged. Supported basic label and TextEdit
styles, plus non-underlined TextEdit halos, use this spacing policy.

Underline/halo combinations now combine CPU-specific guest strokes with the
original outline before smear/exclusion. Classic preserves per-character
descender gaps and its Everything-style row restrictions; PPC includes the
run-wide ribbon in each glyph effect buffer. Binary-recipe and smooth-mask
tests pass; composed appearance and interaction remain unqualified.
Ratio-scaled strikes and bitmap-only sources still need faithful smooth
support. Their existing fallback remains part of the GPUI frontend. Condensed/extended support has native recipe and smooth-mask tests,
guest-driven editing checks, and selected two-line composed captures across
four CPU/display modes at four scales. Native-pixel field crops from all32
cases were directly reviewed; this scope excludes the field's right edge and
full scene. Actual GPUI pointer routing and typing/backspace pass all32
condensed/extended CPU/depth/scale scenarios. Each style also completes192
single-line caret/selection/activation captures:48 restoration pairs and48
visibility checks pass, with one device pixel allowance around native change
bounds. Exactly two full composed state images per style were directly
reviewed. Broader editing and scrolling, physical host input and independent
native-font comparison remain unfinished.

## Evidence and completion gate

Both live presentation and headless captures use the shared compositor and
input transforms. Evidence must distinguish source resolution, native paint
qualification, actual composed appearance, guest interaction and physical host
integration. A preflight pass alone does not prove smooth pixels were painted.

Current captures sample original-font plain and styled menus and qualified
TextEdit/list fields across mono68k, colour68k, PPC8 and PPC16. Regressions check
native advances, atomic fallback, clipping, selection/caret ordering and guest
input. The italic edge test covers raster2–8 with conserved row coverage and
unchanged native envelope. Coverage records state the exact CPU, scale, state,
font/style and visual-review scope, including older renderer versions.

Full qualification still requires broader fonts/styles and state matrices,
editing/scrolling and lifecycle behavior, native font references where
available, host composition/accessibility, and production performance. GPUI
being the default frontend does not establish that release gate.
