# CopyBits spans with a saved outline source

## Cause and change

SC2K periodically copies a 784×545 indexed image during Fire. The call is an unscaled 8-bit `srcCopy`, with palette translation and a complex visibility region. Outline presentation saves the source first to preserve text detail. The shared rectangular transfer path declines complex regions, and the existing region-span helper also declined every saved source. The resulting pixel loop repeated coordinate calculations, region tests and color-transfer decisions for every pixel. A gated diagnostic measured about 26–29 ms for the call, with roughly 0.15 ms spent on setup and source capture. Those diagnostic timings are attribution, not release benchmark results.

Both CopyBits adapters now pass the already captured `SavedPixels` and its first source row to the existing span helper. The helper checks snapshot coverage before writing, intersects the same region rows, and copies selected saved pixels in the same top-to-bottom, left-to-right order through the existing palette table and `copy_saved_pixel` operation. Incomplete snapshots decline without partial writes. Scaling, other depths/modes, unsupported geometry and diagnostic fallback behavior retain their existing paths.

A plain memory copy would lose outline detail; taking a fresh source row after writing an earlier overlapping row could read already modified pixels. Retaining the existing full snapshot and per-pixel memory/presentation operation preserves those behaviors while removing repeated geometry and mode work. The change does not skip writes, alter write notifications, change guest budgets, change JIT compilation, or move work to another thread.

[Inside Macintosh: Imaging with QuickDraw, CopyBits](https://dev.os9.ca/techpubs/mac/QuickDraw/QuickDraw-166.html) specifies destination clipping and the current-port visibility/clip intersection. The existing region representation and palette-transfer table are reused.

Source history explains the missed interaction: 73e7c7f8 (#963) introduced the row-span helper with the no-snapshot gate; f1828269 (#1524) later required source capture for outline presentation even for nonoverlapping images. This is a source-level explanation, not a historical timing bisect.

## Validation method

The standalone branch is based on upstream master `613728f7bde2e2b3f31f0bc0c8d904ac01f409ac`. The initial exploratory replay held pending presentation changes #2113/#2119 constant on both sides. The final master comparison excludes both from both executables and uses the normal compact export path.

Native Windows release executables run the same fixed-clock 2,500-frame SC2K replay in before/after/after/before order. Ordinary city is frames 850–999; Fire is frames 1300–2499. The 50 periodic Fire frames have frame number modulo 24 equal to 8. Scripted input, instruction/tick budgets and guest audio servicing are identical. The replay performs composition and compact export but has no displayed window or GPU submission.

Frame phase durations are elapsed wall time. Windows process CPU counters include the entire replay, startup and four checkpoint captures; they are not a live GUI CPU percentage. RAM captures use Windows-local temporary storage. Builds/tests are held during timing, power conditions are recorded before and after each run, and a lightweight compiler-process monitor checks for accidental overlap.

Correctness compares every non-timing frame field: instruction counts, foreground work, guest tick, PC, total instructions, event count and tracking state. At frames 999/1499/1999/2499 it compares hashes of complete 128 MiB RAM captures plus CPU registers, PNGs and compact presentation buffers. Game/archive bytes are not included in this report.

## Results

The final master replay preserves all 2,500 non-timing records and all four complete RAM/register/image/compact checkpoints in every run. Windows reported battery power and Balanced: 31% for pair A and 30% for pair B. Its battery-saver API flag read off, but the user subsequently confirmed that power-saving mode was on. The API flag does not reliably describe the observed mode; the exact transition time was not captured. The one-second compiler/test-process monitor recorded no overlap.

Absolute runtime increased sharply between the two pairs, including unchanged composition and the ordinary Fire frames. A power-saving transition is a plausible explanation, supported by the user observation, but its exact timing was not instrumented. Accordingly, report the pairs separately rather than treat them as a single stable timing population. Pair A ran before→after; pair B ran after→before.

| Metric | Pair A before | Pair A after | Change | Pair B before | Pair B after | Change |
|---|---:|---:|---:|---:|---:|---:|
| Process CPU, complete replay (s) | 23.078 | 21.234 | -8.0% | 37.125 | 35.438 | -4.5% |
| Periodic Fire guest phase, mean (ms) | 26.392 | 13.965 | -47.1% | 50.160 | 25.807 | -48.6% |
| Periodic Fire complete measured work, mean (ms) | 37.318 | 25.034 | -32.9% | 67.366 | 43.533 | -35.4% |
| All Fire guest phase, mean (ms) | 5.957 | 5.329 | -10.5% | 10.674 | 9.771 | -8.5% |
| Other Fire guest frames, mean (ms) | 5.068 | 4.953 | -2.3% | 8.957 | 9.074 | +1.3% |
| Fire composition, mean (ms) | 1.674 | 1.656 | -1.1% | 2.652 | 2.620 | -1.2% |
| All Fire complete work, p99 (ms) | 39.565 | 27.294 | -31.0% | 70.931 | 47.554 | -33.0% |
| All Fire complete work, maximum (ms) | 54.111 | 62.426 | +15.4% | 93.456 | 64.720 | -30.7% |

The recurring stall improves by 47–49% while the other Fire guest frames change −2.3%/+1.3% and composition changes about −1.1%/−1.2%. This supports attributing the recurring-copy reduction to the patch. Process CPU falls 8.0%/4.5% in these pairs, but startup/checkpoint costs, environmental variation and uncertain power-mode transition timing make those indicative observations rather than a precise gameplay improvement estimate. The earlier controlled presentation-stack ABBA likewise cut the periodic guest phase about 49.7% and reduced complete-replay process CPU about 5.6%.

Pauses remain: the affected complete frames still average 25.0 ms (pair A) or 43.5 ms (slower pair B), and the after runs include complete-frame maxima of 62.4/64.7 ms. The first pair’s maximum increased despite lower p99; the patch is not a universal worst-frame or input-latency guarantee. No outliers were removed from these statistics.

Build provenance (frozen release library SHA-256):

- Master: `956817f37f974af2cdbb0e6db9bce02d83ede28749e2e5bd508cf8fe6edbab02`.
- Master plus this patch: `b6beab523bcf8ab54806f35a9fa816ddfa4c71e607319e2fc08c0591b8b8cf29`.

## Tests and limits

`cargo test --lib --features test-support`: 5,536 passed, three existing tests ignored. `cargo check --locked --no-default-features`: passed. Both native Windows release builds and their replay executables compile.

The new differential test compares the span helper with the former pixel operation for overlapping destinations, identity and translated palettes, nonzero snapshot first rows, shifted bitmap origins/rectangles, padded source strides, live source mutation after capture, and complete outline-image equality. It asserts that the source actually contains outline detail. Both CopyBits entry paths are also exercised with complex clipping, outline presentation enabled/disabled, and shared/separate source and destination bases. Another test confirms incomplete snapshots decline before any write. Existing clipping, palette, overlap and retained-dialog tests also pass.

This removes one periodic source of expensive work, not all pauses. Memory/presentation writes, composition, JIT execution and synchronous compilation still cost CPU. Hidden replay timing does not measure displayed frame pacing or menu/cursor latency.

Follow-up review coverage: all 735 QuickDraw tests pass after those additional cases; production code and the measured executables are unchanged.
