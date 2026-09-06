# Outline font Toolbox Showcase

These captures run the actual 68k Toolbox Showcase with the default bundled
TrueType fonts, Classic System 7 theme and a fixed startup clock. The guest
screen remains 800 × 600 at 8-bit depth. The desktop uses the same 4× surface
initialization as the capture test.

| Page | 2× (1600 × 1200) | 4× (3200 × 2400) |
| --- | --- | --- |
| TextEdit | [Open](2x/textedit.png) | [Open](4x/textedit.png) |
| Styled text | [Open](2x/styled-text.png) | [Open](4x/styled-text.png) |
| Drawing | [Open](2x/drawing.png) | [Open](4x/drawing.png) |
| Controls | [Open](2x/controls.png) | [Open](4x/controls.png) |
| Graphics | [Open](2x/graphics.png) | [Open](4x/graphics.png) |

<img src="4x/textedit.png" alt="Actual Toolbox Showcase TextEdit with 4× outlines" width="800">
<img src="4x/styled-text.png" alt="Actual styled TextEdit runs with sharp italic and underlined outlines" width="800">

## Reproduce

```sh
./tests/toolbox-showcase/build.sh --verify
SYSTEMLESS_FONT_EVIDENCE_DIR=tests/toolbox-showcase/outline-fonts \
cargo test --no-default-features --test toolbox_showcase \
  capture_outline_font_showcase -- --ignored --nocapture
```

The capture test repeats five pages with presentation disabled, at 2× and at
4×. Every enlarged run must preserve the fresh baseline's guest framebuffer,
execute real outline draws, and differ from nearest-neighbor enlargement.
TextEdit contents, style state and measured-width bars are also checked.
Only the ten lossless gallery PNGs are generated.

The complete interaction test passes separately on both 68k and PowerPC:

```sh
cargo test --no-default-features --test toolbox_showcase test_toolbox_showcase -- --exact
SYSTEMLESS_PREFER_POWERPC=1 cargo test --no-default-features \
  --test toolbox_showcase test_toolbox_showcase -- --exact
```

The [guest-resolution reference gallery](../README.md#reference-screenshots)
was regenerated for both architectures. Changed metrics intentionally affect
wrapping and selection: this TextEdit sample wraps into five lines, and its
fixed mouse drag selects 16 characters. Layout probes sample borders and
backgrounds rather than relying on the old font's ink at a particular pixel.
The archive rebuilt byte-for-byte with the cached pinned toolchain image;
Docker's registry metadata lookup was unavailable.

## Rendering scope

Bundled URW and Noto outlines replace the hand-drawn font catalogue. Guest
bitmap/outline resources and explicit local overrides retain precedence.
See [URW provenance](../../../src/quickdraw/fonts/urw/README.md) and
[Noto provenance](../../../src/quickdraw/fonts/noto/README.md).

The 4× desktop surface handles plain, bold, italic and underlined 68k
srcCopy/srcOr text on 8-bit screens, plus shared chrome and outline menu
symbols. The guest's binary framebuffer and text advances remain unchanged
by presentation. Visibility and clipping regions constrain the enlarged ink;
repeated coverage does not darken edges, and opaque text runs preserve
adjacent overhangs. Cursor and debug overlays retain guest coordinates.

Native PowerPC drawing, other pixel depths, remaining styles/transfer modes
and browser output use the logical raster. Bitmap copies, selection inversions
and palette changes can discard extra outline detail until text is redrawn.
Substituted font metrics can also overflow fixed layouts in guest applications;
increasing presentation resolution does not change those layouts.
