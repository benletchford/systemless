# URW Toolbox Showcase review

These are regenerated captures of the actual 68k Toolbox Showcase, using the
Classic System 7 theme and a fixed startup clock. The guest screen is 800 × 600
at 8-bit depth; supported URW text is presented at 2× or 4×. Open a capture to
inspect its full resolution. The [default renderer gallery](../README.md#reference-screenshots)
remains the baseline for normal builds.

| Page | 2× | 4× |
| --- | --- | --- |
| TextEdit | [1600 × 1200](2x/textedit.png) | [3200 × 2400](4x/textedit.png) |
| Drawing | [2×](2x/drawing.png) | [4×](4x/drawing.png) |
| Controls | [2×](2x/controls.png) | [4×](4x/controls.png) |
| Graphics | [2×](2x/graphics.png) | [4×](4x/graphics.png) |
| Styled text | [2×](2x/styled-text.png) | [4×](4x/styled-text.png) |

<img src="4x/textedit.png" alt="Toolbox Showcase TextEdit with 4× URW outlines" width="800">

## Reproduce

From the public repository:

```sh
./tests/toolbox-showcase/build.sh --verify
SYSTEMLESS_FONT_EVIDENCE_DIR=tests/toolbox-showcase/urw-experiment \
cargo test --no-default-features --features experimental-urw-fonts \
  --test toolbox_showcase capture_urw_font_experiment -- --ignored --nocapture
```

The capture test first runs with presentation disabled, then repeats the same
five pages at 2× and 4×. It checks each guest framebuffer against that fresh
in-memory baseline, checks live TextEdit/style state, and requires native output
to differ from nearest-neighbor enlargement. It writes only the ten gallery
PNGs, with lossless compression; no historical snapshots or image-processing
scripts are required. The accepted default-renderer reference images are not
updated by this test.

## Review scope

The optional fonts use the original URW files and preserve guest-resource and
local-override precedence; [source and licences](../../../src/quickdraw/fonts/urw/README.md)
are recorded separately. Logical metrics differ from the default bitmap fonts.
The presentation layer preserves those guest metrics while rasterizing at
physical resolution. Repeated coverage is stable; opaque text runs protect
adjacent glyph overhangs from per-character background erasure.

This remains an experimental capture API, not an enabled desktop/browser
high-DPI backend. Plain/bold srcCopy/srcOr text and the shared 8-bit chrome
painter are covered. Other styles, resource fonts, bitmap content, and transfer
modes retain their existing appearance. Copies/scrolls lose extra detail until
redraw. PowerPC presentation, other pixel depths, changing palettes, selection,
and retained offscreen surfaces need further work. Increased resolution does
not resolve layout overflow caused by substituted font metrics.

## Validation

The guest archive rebuilt byte-for-byte using the cached toolchain image;
Docker's registry metadata lookup was unavailable. The regenerated ten images
match the last reviewed pixels. The gallery test, default-renderer interaction
test, WebAssembly build, and package inventory pass.

The default library suite passes 5,117 tests (3 ignored). With experimental URW
fonts enabled, 5,117 pass and the following 7 fail (3 ignored). These remain
review blockers for adopting the fonts as the default; no assertions or default
reference pixels were relaxed.

```text
loader::ppc::tests::menu_bar_title_baseline_tracks_the_live_menu_bar_height
loader::ppc::tests::short_menu_bar_system_mark_does_not_bleed_below_the_bar
menu_manager::tests::standard_menu_text_measurement_is_shared_between_gateways
trap::framebuffer::redraw_chrome_tests::menu_bar_title_baseline_tracks_live_height
trap::framebuffer::redraw_chrome_tests::redraw_chrome_places_live_popup_over_the_kiosk_stage
trap::menu::tests::systemless_theme_does_not_change_popupmenuselect_geometry_and_result
trap::quickdraw::tests::stringwidth_scales_monotonically_with_textsize
```
