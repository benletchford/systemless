# Coppet: a Geneva 9 optical-size substitute

Coppet is an Inter-derived font fitted to classic Geneva-compatible widths.
This first version is **primarily optimized for 9 pt text**. Systemless uses
its 95 printable ASCII glyphs only for unresolved Geneva/Application requests
at 9 points. Larger point sizes and extended Mac Roman characters continue
using the existing URW fallback. Larger sizes may need further optical tweaks
before Coppet is enabled there. Resizing the host window still uses the same
9-point design at the retained presentation resolution.

The wider classic advance cells made the previous Nimbus Sans substitute look
unevenly spaced in SC2K phrases such as “Power Plant Needed.” Coppet fits its
ink to those cells, then adjusts counters, bearings, stroke weight and pixel
alignment. No kerning, tracking, or changes to the game's pen advances are
added. [Inside Macintosh distinguishes image bounds from logical widths](https://dev.os9.ca/techpubs/mac/Text/Text-250.html),
and [integer widths are the compatibility default](https://dev.os9.ca/techpubs/mac/Text/Text-197.html).
GetFontInfo metrics remain unchanged. The application's own FONT/NFNT/
sfnt resources and explicit local bitmap overrides retain precedence.

## License and provenance

**Coppet, its editable TTX source and its font build script are Font Software
under the SIL Open Font License 1.1, not the emulator's GPL license.**
See [OFL.txt](OFL.txt), also shipped as `Coppet-OFL.txt` in release archives.
Preserve that notice when redistributing the font.
Modified fonts must remain under OFL-1.1; bundling with Systemless is permitted.
The OFL's restrictions on selling the font by itself and reserved font names
still apply. The emulator integration code remains GPL-3.0-or-later.

Source components:

- **Inter:** Copyright 2020 The Inter Project Authors. Outlines from
  [`Inter[opsz,wght].ttf`](https://github.com/google/fonts/blob/1ac2012c34919f5fa2675aacf723fa98edb30b5f/ofl/inter/Inter%5Bopsz%2Cwght%5D.ttf),
  SHA-256 `29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031`.
  Most glyphs start at weight 400 / optical size 14; selected lighter strokes
  use weight 360. The curved-foot l starts from Inter's `l.ss02` alternate.
- **Historical Kurrajong 9 fitting data:** Copyright (c) 2026 Ben Letchford,
  Reserved Font Name “Systemless,” OFL-1.1. Advances and initial ink boxes came
  from [`geneva9.rs`](https://github.com/benletchford/systemless/blob/0540ef827d800fbdb00eafc67922cef3245b81e5/src/quickdraw/fonts/pixel_font/geneva9.rs),
  SHA-256 `2578381b3fcf9f3bde359d1b258bcf0b95fcff8ea3181863d0c395c7563bfada`.
  Kurrajong's bitmap contours were not traced into Coppet.

Both source copyright notices are retained. **Coppet** is the derivative's
primary family name; classic “Geneva” and “Application” names are compatibility
identifiers only. No Apple font binary, outline or bitmap artwork is included
in this font. Original Geneva was viewed as a separate local reference.

The refinement history is preserved in the
[comparison branch at 2c03af9](https://github.com/rlanday/systemless/tree/2c03af9/experiments/geneva9-fonts).
The TTX file here is the editable final outline source, so rebuilding the
shipped font does not require that experimental branch or downloading fonts.

## Rendering and remaining work

Skrifa hints the outline at 9 ppem for the guest's binary QuickDraw mask and
at 36 ppem for Systemless's retained grayscale presentation. Window resizing
uses the existing single-pass coverage resampling. Positioning a thin stroke
inside a native pixel column/row avoids splitting its coverage unnecessarily.
This is a font design change, not a new antialiasing backend.

The accepted version refines lowercase spacing and weights, adds a short
entry stroke to i, retains a curved-foot l, aligns the T stem and R middle
bar, and opens the ampersand's upper counter. The 9-ppem l instruction keeps
its curved foot from disappearing in the guest bitmap; it does not affect
36-ppem retained rendering. ASCII artwork intentionally changes in the guest
as well as on the host, so unchanged metrics alone do not imply pixel-identical
application behavior. Mixed ASCII/extended-character runs retain URW accents
and can show a design difference; expanding character coverage is future work.

Future refinements can improve larger optical sizes and remaining glyphs.
The current scope does not claim a complete Geneva recreation or a finished
multisize family.

## Rebuild

Install FontTools 4.65.0 and ttfautohint 1.8.4, then run `python build.py`.
`TTFAUTOHINT` can select the tool executable. The TTX source has deterministic
timestamps. Automatic x-height enlargement is disabled; instructions are
regenerated after outline changes. Cargo embeds the checked-in TTF and does
not run the font build or use host fonts.

Version 0.024 SHA-256:

```
039de58d710483aad4110106677a46bd39d822204542520470b2becdcda30afb  Coppet-Regular.ttf
```

The font's full OFL notice is also embedded in its name table. The shipped
outline/hint/metric tables match the accepted comparison font exactly; only
name/license metadata and the resulting head checksum differ.
