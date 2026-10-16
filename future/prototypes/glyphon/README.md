# Glyphon compatibility probe

This isolated executable evaluates Glyphon 0.12.0 with the rebuild's wgpu 30.0.1
and classic FONT decoding. It is not a production backend and does not select a
renderer for the runtime. The production manifest and lockfile are unchanged.

From the repository root, run:

```sh
cargo run --locked --manifest-path future/prototypes/glyphon/Cargo.toml
cargo +1.93.0 check --locked --manifest-path future/prototypes/glyphon/Cargo.toml
cargo +1.99.0 clippy --locked --manifest-path future/prototypes/glyphon/Cargo.toml -- -D warnings
cargo deny --manifest-path future/prototypes/glyphon/Cargo.toml --config future/prototypes/glyphon/deny.toml check licenses
```

A real wgpu adapter is required. Adapter or GPU failures are errors, not skipped
successes. The program prints the adapter and checks exact RGBA readback on a
16-by-12 offscreen target. Its synthetic FONT fixture is decoded by the actual
shared `systemless_future::font::Strike`; Glyphon receives the resulting one-bit
mask through `prepare_with_custom`. The host font database is empty, and
`Buffer::new_empty` avoids fallback shaping. No native font APIs render pixels.

The six cases verify or characterize:

1. Exact bitmap mask pixels and physical-pixel snapping at a fractional placement.
2. Cache reuse without another rasterization request.
3. Partial rectangular clipping.
4. An edited source FONT byte with the same custom-glyph key retains cached pixels.
5. A new custom-glyph ID refreshes pixels from the edited FONT data.
6. Repeating the stock renderer preserves white pixels rather than implementing
   QuickDraw XOR, which would cancel the two identical draws.

Cases 4 and 6 deliberately characterize missing adapter semantics. They do not
claim guest-edit invalidation or QuickDraw XOR is already implemented by Glyphon.

## Recorded result

On 2026-10-16 all six cases passed on an Apple M1/Metal adapter. Rust 1.93 check,
strict Rust 1.99 Clippy and the scoped license gate passed. Logs:

- `/tmp/systemless-glyphon-probe-acceptance.txt`
- `/tmp/systemless-glyphon-msrv-acceptance.txt`
- `/tmp/systemless-glyphon-lint-acceptance.txt`
- `/tmp/systemless-glyphon-licenses-acceptance.txt`

Earlier exploratory builds exposed the fallible wgpu buffer-view API, strict
slice lint requirements and the default text constructor's fallback-font lookup.
Those were corrected; earlier failed runs are superseded, not counted as passes.

Glyphon's license is MIT OR Apache-2.0 OR Zlib. The pinned direct graph includes
cosmic-text 0.19.0, etagere 0.3.0, lru 0.16.4, rustc-hash 2.1.3 and wgpu 30.0.1.
The separate lockfile makes this experiment reproducible. The license configuration
uses a subset of the repository's permissive allowlist, strict unused entries and
scoped exceptions for the two local GPL packages.

## Next integration gate

The result supports a bitmap-mask adapter prototype. Before production adoption:

- Preserve guest metrics/layout and assign bounded cache identities tied to font
  content/selection/generation. Handle edits, moves, replacement and ID exhaustion.
- Render masks on the GPU, then apply the runtime's exact QuickDraw transfers,
  colors, clipping and packed indexed destination semantics on the GPU. Keep
  readback in tests; the production bridge must not copy mask pixels through the
  CPU merely to perform destination composition.
- Verify 68K/PPC guest entry points, synchronization with guest pixel writes,
  opaque spaces, style/spacing APIs and actual UI callers.
- Benchmark complete workloads including uploads, atlas growth, GPU passes and
  readback/synchronization costs; this small probe is not a performance benchmark.
- Verify native presentation and browser support separately.

Primary API references:
[Glyphon TextRenderer 0.12.0](https://docs.rs/glyphon/0.12.0/glyphon/struct.TextRenderer.html),
[COSMIC Text Buffer 0.19.0](https://docs.rs/cosmic-text/0.19.0/cosmic_text/struct.Buffer.html),
and [custom glyph source](https://github.com/grovesNL/glyphon/blob/main/src/custom_glyph.rs).
