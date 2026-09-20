# Reusing unchanged compact GPU frames

The Windows presenter still rebuilds its compact GPU input arrays when the retained visible image has not changed. Cache those arrays using an owned surface identity and a visible-image revision, while retaining the existing guest execution, window composition, GPU upload, and submission behavior.

## How this differs from #2113

Systemless retains the logical framebuffer pixels and extra samples for high-resolution outline text. The GPU transport has one entry per logical pixel: either an RGB color or an offset into the retained samples. Before #2113, preparing this transport also generated a logical ARGB image, copied it as an overlay reference, checked opacity, and compared the images to identify software overlays. With a native host cursor and no debug overlay, those images were identical by construction.

#2113 removes those intermediate images and comparisons. It still visits every logical pixel and rebuilds the compact arrays for each prepared frame, including an unchanged screen. At 800×600 that means visiting 480,000 logical cells plus retained text samples; the baseline in this comparison measured about 0.8 ms for that export.

This follow-up avoids rebuilding the arrays when they already describe the current visible image. After the normal composition pass, an unchanged visible-image stamp lets the frontend reuse the prepared arrays. A changed stamp triggers the same complete export used by #2113. Thus #2113 makes each export cheaper; this change reduces how often export is necessary. Performance comparisons use #2113 as the baseline, so any improvement is additional rather than counting its savings again.

| Work for an eligible frame | Before #2113 | With #2113 | With this follow-up |
| --- | --- | --- | --- |
| Build logical ARGB image, clone it, check for overlays | Every frame | Omitted | Omitted |
| Build compact cells and retained-text sample arrays | Every frame | Every frame | When the visible-image stamp changes |
| Guest execution and window composition | Existing behavior | Existing behavior | Existing behavior |
| GPU upload and submission | Existing behavior | Existing behavior | Existing behavior |

For example, if the game computes its next simulation step without changing the visible image, #2113 still rebuilds the GPU input arrays. This follow-up can reuse them. If the game changes a city tile, draws a menu, updates retained text, or changes an indexed color, the next preparation exports the current image before submitting it. A host window resize can reuse the same logical image while presenting it at the new output size.

Existing retained-text and saved-pixel caches preserve drawing information, and the existing software presentation cache stores a software image. They do not supply an already prepared `CompactPresentation` to the Windows GPU frontend. The new cache owns that particular output, so it avoids rebuilding a different representation of data already retained elsewhere.

The stamp is maintained at the existing mutation points, without hashing or scanning the full screen. Visible byte writes, retained coverage changes and indexed-palette changes invalidate it. Offscreen-only drawing does not; copying that content onto the visible screen does. Equal plain writes and equal retained-detail restores keep their existing early returns. Invalidation is conservative: an erase/redraw sequence or palette transition may trigger export even if the final displayed pixels are identical.

## Cache ownership and correctness

`CompactPresentationCache` owns the compact arrays and their source stamp. Immutable access supports upload and deferred resubmission. Mutable access clears the stamp before a caller adds software overlays or replaces the arrays, so returning from a software cursor/debug overlay to the retained-image path cannot reuse overlay pixels. Independent cache consumers keep their own stamps; no consumer clears a shared dirty flag.

Each retained surface has an owned identity, so a replacement cannot match an old cache merely because its geometry and revision are equal. The identity also changes if the revision wraps. The existing global revision continues to advance for offscreen mutations; the visible-image stamp records only mutations relevant to this cache. Drawing bookkeeping and the established dialog/software caches retain their original global-revision behavior.

The primary additional risk is stale output from a missed invalidation. Tests cover logical writes, retained coverage without a RAM write, detail erasure/restoration, offscreen work followed by visible copies, palettes, size/depth/scale and surface replacement, mutable output access, independent consumers and revision wrap. GPU failure still reconstructs current software pixels through the existing fallback.

This change reuses CPU-prepared input arrays. It does not skip guest execution or composition, alter instruction budgets, skip GPU uploads, change shaders, or queue an additional frame.

## Validation and performance

The baseline is #2113 at `a498558d`, on upstream `1444ee3c`, including Coppet, the merged GPU work and `m68k 0.14.0`. Four separate native Windows release runs execute the same 2,500-frame SC2K replay in before/after/after/before order, with isolated archive copies. No builds, tests or other diagnostics ran during the timing intervals.

| Controlled replay | #2113 baseline | This follow-up | Reduction |
| --- | ---: | ---: | ---: |
| Ordinary city: compact export per frame, mean | 0.813 ms | 0.079 ms | 90.3% |
| Fire: compact export per frame, mean | 0.796 ms | 0.080 ms | 89.9% |
| Complete replay: total process CPU, mean | 13.023 s | 11.109 s | 14.7% |

The export means include cache hits and full rebuilds. Individual complete-replay CPU reductions were 13.2% and 16.2%. The replay has no displayed GPU submission or live audio, so these figures are additional CPU-work savings over #2113, not live GUI CPU usage, input-to-display latency, or evidence of reaching the 7%-of-one-core target. The earlier dirty-tile diagnostic is not a performance measurement for this change.

- All 2,500 non-timing frame records match in all four runs. Four checkpoints match complete 128 MiB RAM hashes, CPU registers, rendered PNGs, and binary compact cells/detail samples.
- A separate diagnostic compared cached output with a freshly exported reference on every one of the 2,500 replay frames; width, height, scale, cells and detail samples all matched. Its extra validation overhead is excluded from the timings.
- 5,531 library tests passed (3 ignored); 62 Linux desktop tests with `debug-server` passed; native Windows release library and production frontend built.
- Five hidden Windows frontend checks passed: immediate/deferred injected GPU failures at 800×600 and 1104×828, and immediate failure at 500×375. They exercise plain/software-cursor/debug/both-overlay transitions and resizing. Actual software fallback matched an independent current-frame reference after deliberately poisoning the stale logical image with one magenta pixel.

Displayed Windows production checks also passed for the city at native and enlarged sizes and Fire at native size. Actual desktop captures verified the visible scene at both ends of every interval, with no input interference, minimization, or newspaper/budget dialog. Both builds used the GPU presenter. The Disasters menu opened and started Fire in both builds; the cached build also opened and dismissed the menu during Fire without leaving stale menu pixels.

| Live GUI sanity check: total CPU as % of one core | #2113 baseline | This follow-up |
| --- | ---: | ---: |
| Ordinary city, 800×600 | 29.7% | 24.7% |
| Ordinary city, 1104×828 | 28.8% | 23.9% |
| Fire, 800×600 | 47.9% | 43.7% |

Each interval lasts approximately 12 seconds and includes all process threads, GPU submission and live audio. These are one before/after pair, with different fresh-city terrain, dates and Fire locations; they support the direction of the controlled result rather than a precise matched-scene speedup. Recorded power conditions matched at the start and end of both runs. CPU usage remains above the roughly 7%-of-one-core target.

The menu check is functional, not a numerical input-to-display latency measurement, and does not rule out rare stalls. Enlarged Fire, current EV Override GUI behavior, and real device-removal recovery have not been validated for this cache.


## Measurement details

The complete replay CPU times in execution order were 13.140625, 11.406250,
10.812500 and 12.906250 seconds (before, after, after, before). Ordinary-city
phase statistics use frames 850–999; Fire uses frames 1300–2499. The process
counter includes startup and checkpoint overhead; per-phase timers measure
elapsed time. Neither quantity is divided by the host's logical CPU count.

For context, mean Fire guest CPU execution time was 4.136 → 4.094 ms and
composition was 1.175 → 1.222 ms per frame. Ordinary composition was
1.270 → 1.222 ms. This change claims savings from compact export and the
complete process counter, not a speedup of unrelated phases. The visible
stamp adds bookkeeping at mutation sites; its cost is included in the
complete-run measurements.

A diagnostic comparison of cached versus uncached arrays ran separately,
so its fresh exports and equality checks do not contribute to the timed
candidate. All four RAM/register/image checkpoints and non-timing frame
records match the baseline. The five hidden fallback checks likewise ran
before timing. Production GUI executables contain none of these diagnostic
checks or forced failures.

## Deliberate boundary of this change

A changed stamp still causes a full export; there is no per-tile export or
partial GPU upload. Conservative invalidation is allowed to rebuild an
image that eventually returns to the same pixels. The optimization never
skips guest work because the screen appears static. In particular, Fire
continues executing at the existing instruction budget.

Avoiding repeated GPU uploads or composition is separate work: it would
need its own correctness and timing checks, including window resizing,
deferred submission, exposure, and device recreation. This cache alone
neither drops presentation requests nor changes queue depth.
