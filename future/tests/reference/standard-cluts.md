# Standard color table comparison fixtures

These are Systemless comparison data, not a new native System 7 or ROM oracle.
They were generated from the preserved baseline checkout at commit
634c0e9120a695224d71c621fbbe33709670d707, without running or linking the legacy
runtime. The isolated generator compiled its pure palette function/constants
and serialized a big-endian ColorTable: seed=depth, flags=0, size=count-1,
then sequential ColorSpec values and sixteen-bit RGB components.

- `clut4-systemless.bin`: 136 bytes; the `COLORS` array inside
  `standard_mac_indexed_clut` in `src/systems/macintosh/trap/dispatch.rs`.
  SHA-256: b4ecdeb21fcb43c85aa4e6e9645aa959f9e5aceef726a7b67a1413d470fa2afe
- `clut8-systemless.bin`: 2056 bytes; `standard_mac_8bpp_clut` in
  `src/systems/macintosh/display.rs`.
  SHA-256: ea0c77ffd9e946f9877009a414263671c5511ae4e60cbf0d71503730ff302e49

The generator source for this capture is `/tmp/systemless-baseline-clut.rs`.
The fixtures intentionally contain no display RGB332 policy or resource-handle
addresses. Full byte comparisons catch ordering, ramp endpoints, precision,
metadata and off-by-one changes. Keep these fixtures separate from the new
runtime's generator so its output cannot silently redefine its reference.

Apple's [Palette Manager default-table documentation](https://dev.os9.ca/techpubs/mac/ACI/ACI-20.html)
defines base resource IDs 1/2/4/8 and grayscale IDs 34/36/40; the captured legacy
values provide the separately labelled exact-byte comparison. Highlight-adjusted
IDs and native conformance are not established by these fixtures.
