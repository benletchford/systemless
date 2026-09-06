# Experimental URW fallback: Toolbox Showcase

This experiment addresses [#673](https://github.com/benletchford/systemless/issues/673)
without changing the default renderer. These are screenshots from the real
Toolbox Showcase guest application, using Classic System 7 chrome, a fixed
startup clock, the 68K executable, and an 800 × 600, 8-bit screen. Both runs use
the same fixture and capture routine. Images are unmodified framebuffer exports.

| Page | Default bitmap fallback | Experimental URW fallback |
| --- | --- | --- |
| Graphics | ![Before graphics](before/graphics.png) | ![After graphics](after/graphics.png) |
| Drawing and typography | ![Before drawing](before/drawing.png) | ![After drawing](after/drawing.png) |
| Controls | ![Before controls](before/controls.png) | ![After controls](after/controls.png) |
| TextEdit | ![Before TextEdit](before/textedit.png) | ![After TextEdit](after/textedit.png) |
| Styled text and measurements | ![Before styled text](before/styled-text.png) | ![After styled text](after/styled-text.png) |

## What the comparison establishes

The baseline is public `master` commit
[`e2772dfd4a7fc8890c69bffeea6f9991f21427ae`](https://github.com/benletchford/systemless/commit/e2772dfd4a7fc8890c69bffeea6f9991f21427ae)
with the capture helper added. The experimental run enables the feature on
the same source and fixture.

- The TextEdit page's right-hand headings fit inside their wells instead of
  crowding the right edge. The 208-byte live buffer wraps to five lines instead
  of six. The centered callout also occupies fewer lines.
- The drawing page's sample measures 77 pixels instead of 82. This is an
  observable change in font advances, not a claim of better classic fidelity.
- The font tests show exact 17- and 40-point strikes instead of scaling a nearby
  hand-authored bitmap, binary glyph masks, and extended
  Mac Roman lookup. Courier's 20-point advances remain uniformly 12 pixels.
- All five pages complete on both 68K (8-bit) and PowerPC (16-bit). The styled
  text probe verifies colored runs and three measurement bars on both.

The visual benefit is a consistent outline-derived fallback with more compact
layout in these examples. It is **not a universal visual improvement**: small
monospaced text is still coarse, letter shapes differ from the original Mac
faces, and narrower advances can change layouts that already fit correctly.
Core 35 does not provide exact Chicago, Geneva, Monaco, London, or Cairo designs.
This experiment is not native-Mac pixel approval.

## Acceptance limits

The default library tests pass. The experimental font-contract tests and both
showcase capture runs pass, but the full library suite with the feature enabled
still exposes classic menu/pixel/spacing assumptions. Those checks have not been
weakened or had their expected pixels updated. Three tests of bitmap-specific
family substitution/scaling remain enabled only for the default backend.

Validation on the final experimental renderer:

| Check | Result |
| --- | --- |
| Default library suite (`--no-default-features --lib`) | 5,117 passed; 3 ignored |
| Experimental font contracts (`--features experimental-urw-fonts --lib quickdraw::fonts`) | 25 passed |
| Full experimental library suite | 5,107 passed; 11 failed; 3 ignored |
| Showcase captures: 68K before/after and PowerPC after | Passed |
| `wasm32-unknown-unknown` check with the experimental feature | Passed |
| Package file inventory | All eight fonts, source record, and OFL notices included; screenshots excluded |

Remaining failures with the experimental feature:

```
loader::ppc::tests::hle_import_runner_builds_and_draws_mbar_resources
loader::ppc::tests::menu_bar_title_baseline_tracks_the_live_menu_bar_height
loader::ppc::tests::menu_tracking_round_trips_unaligned_popup_boundaries_at_supported_depths
loader::ppc::tests::short_menu_bar_system_mark_does_not_bleed_below_the_bar
loader::ppc::tests::tracked_menu_draws_standard_item_chrome_and_dims_disabled_rows
menu_manager::tests::standard_menu_text_measurement_is_shared_between_gateways
trap::menu::tests::drawmenubar_4bpp_keeps_the_color_system_mark_through_title_reversals
trap::menu::tests::drawmenubar_keeps_the_retro_computer_mark_legible_in_monochrome
trap::menu::tests::drawmenubar_uses_retro_computer_art_for_the_system_mark_without_layout_drift
trap::menu::tests::systemless_theme_does_not_change_popupmenuselect_geometry_and_result
trap::quickdraw::tests::stringwidth_scales_monotonically_with_textsize
```

Before enabling this by default, resolve the menu-mark placement, popup geometry,
and size-scaling failures; review small sizes, reverse video, and application
layouts against a broader native reference set. This does not add guest Font
Manager trap support or change `RealFont`'s existing availability rules. The
prototype therefore references #673 without closing it.

## Reproduce

Run from a checkout of the public systemless repository. Leave
`SYSTEMLESS_ORIGINAL_FONTS_DIR` and `SYSTEMLESS_PREFER_POWERPC` unset for this
68K comparison. These commands write evidence only; they do not accept or
replace the main regression suite's reference images.

```sh
SYSTEMLESS_FONT_EVIDENCE_DIR=tests/toolbox-showcase/urw-experiment/before \
  cargo test --no-default-features --test toolbox_showcase \
  capture_urw_font_experiment -- --ignored
SYSTEMLESS_FONT_EVIDENCE_DIR=tests/toolbox-showcase/urw-experiment/after \
  cargo test --no-default-features --features experimental-urw-fonts \
  --test toolbox_showcase capture_urw_font_experiment -- --ignored
```

The capture waits for each selected page to return to the guest event loop.
It also checks live TextEdit content, colored style runs, and rendered
CharWidth/TextWidth/MeasureText bars. Successful execution proves that these
pages render and their semantic probes pass; it does not establish pixel or
metric equivalence to classic fonts.

The optional backend uses unmodified OFL-licensed URW fonts and Swash hinting,
with 0.10/0.25/0.35-pixel ink expansion for bold/regular/monospaced faces
before 50% thresholding. This retains thin strokes
when QuickDraw uses binary masks. Advances are taken from the font, so ink
expansion does not silently alter measurement. The guest sfnt renderer is
unchanged. See [source and license details](../../../src/quickdraw/fonts/urw/README.md).
