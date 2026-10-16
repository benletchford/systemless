# Complete wgpu text rendering

This is an explicit full-rebuild acceptance requirement added by the user on
2026-10-16. It extends the original objective; it does not replace resource-fork,
CODE/PEF, game compatibility, mixed-mode or other rebuild requirements. The goal
must remain active until this requirement and the rest of the rebuild are verified.

All supported guest text must draw through the shared wgpu graphics system:
application text, TextEdit, menus, dialogs, controls and window decorations. CPU
font decoding, measurement, layout and source-glyph preparation may remain; all
destination pixel operations and composition must use wgpu. Native host text APIs
must not become a second production renderer.

## Current evidence and remaining work

`font.rs` parses guest FONT/NFNT data. `text.rs` prepares disjoint glyph ink and
background coverage and sends it through the same GPU port drawing services.
`textedit.rs` uses shared font/graphics behavior. `font_fallback.rs` currently
rasterizes fallback outlines on the CPU with ab_glyph to prepare packed source
strikes. These facts establish an existing GPU destination path, not completion
of all text APIs, styles, fallback behavior or the library evaluation below.

## Evaluate libraries before extending the custom path

Primary upstream sources inspected on 2026-10-16:

- [Glyphon manifest](https://github.com/grovesNL/glyphon/blob/main/Cargo.toml):
  upstream main declares 0.12.0, wgpu 30 and cosmic-text 0.19. This matches the
  rebuild's wgpu major. Its [README](https://github.com/grovesNL/glyphon/blob/main/README.md)
  describes shaping/rasterization through cosmic-text, glyph atlas packing through
  etagere, and rendering through wgpu. Its
  [custom glyph API](https://github.com/grovesNL/glyphon/blob/main/src/custom_glyph.rs)
  accepts single-channel masks and physical-pixel snapping. Evaluate it first for
  bitmap-strike adapters and batching, plus modern fallback text. Dependency/MSRV,
  cache invalidation, exact masks and QuickDraw transfer semantics still need a
  concrete prototype; no dependency has been selected or added yet.
- [Vello architecture](https://github.com/linebender/vello/blob/main/ARCHITECTURE.md):
  current Vello GPU preprocesses geometry on the CPU and rasterizes/composites on
  the GPU, with text/glyph runs through glifo. The original compute renderer is
  now described as experimental research. Evaluate the current wgpu-backed family
  for outline/fallback glyphs if it fits the compatibility and memory requirements.
  Do not adopt its CPU renderer or WebGL backend as production alternatives.
- Compare both against the current exact mask/coverage path. Reuse a library where
  it improves the result, while keeping classic guest semantics in shared runtime
  services. Record the concrete API fit, versions, licenses, dependency cost,
  performance and any adapter work before choosing or rejecting a library.

These are observations of upstream main, not a claim that releases have been
integrated or tested. Pin any selected version and verify it against current
primary documentation at implementation time.

## Acceptance

1. Audit every guest text entry point and every UI text caller. Implement remaining
   text APIs/styles/spacing deliberately and route their drawing through wgpu.
2. Preserve original bitmap fonts, MacRoman mapping, missing glyphs, bearings,
   advances, style effects and guest-visible measurements. Modern shaping or
   antialiasing must preserve the behavior required by the selected guest font.
3. Preserve port origin, clipping/regions, foreground/background colors, opaque
   spaces and classic transfer operations, including XOR, on packed indexed
   destinations and the production display formats.
4. Keep guest pixel memory coherent across direct guest writes and GPU text draws.
   Font edits, handle movement/replacement and font selection changes must invalidate
   cached glyphs/layout correctly. Both CPU adapters use one font/text state.
5. Verify equivalent 68K/PPC outcomes with deterministic fixtures and GPU readback,
   including low-bit/packed pixels, clipping, styles, spaces, edited font bytes and
   mixed text/QuickDraw operations. Exercise application UI text in real originals.
6. Verify native presentation and a portable browser design with explicit platform
   evidence. Unavailable GPU/browser checks remain unverified, never passed.
7. Measure representative text-heavy workloads before/after any library migration;
   include uploads, dispatches, cache/memory growth and synchronization costs.

The next text milestone is a bounded Glyphon compatibility prototype and API audit,
followed by the chosen integration and pixel-level acceptance. This is additional
work alongside the current original Marathon/Escape Velocity compatibility slices.


## Baseline GPU verification (2026-10-16)

The existing offscreen text fixture passes on a real adapter, covering glyph
transfer modes, clipping and port preservation. Evidence:
/tmp/systemless-getctable-text-gpu.txt, terminal exit 0. The same run passes the
real device PixMap CopyBits/readback check. This proves the currently supported
GPU text slice; API/UI completeness, library prototype/selection, all formats and
styles, native presentation and browser verification remain open.
