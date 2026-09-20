# Direct compact export without software overlays

The Windows GPU presenter already accepts a compact retained image: one RGB
word for a uniform logical pixel, or an offset into its retained outline
samples. Nevertheless, preparing that image also generated a full logical
ARGB image, cloned it, checked opacity, and compared the two images to find
software overlays. With a native host cursor and no debug overlay, the images
were identical and their pixels were not needed by the exporter.

`compact_presentation_without_overlays` exports the same retained cells and
detail samples directly. Both APIs share the export loop; their monomorphized
iterators provide either overlay differences or a constant absence of
overlays. The new entry point checks that the retained surface matches the
current logical dimensions before touching the output. The existing tile
offset bounds check still applies.

The Windows frontend uses this path only while a GPU presenter and visible
outline detail exist, with no software cursor or debug overlay. Other cases
keep the existing image generation and overlay path. This does not add a
frame cache, dirty tracking, a new shader, or a new presentation queue.

## Failure behavior and risks

Skipping logical image generation leaves the reusable logical buffer stale;
it can contain an older screen or have the wrong size. On a same-call GPU
failure, the frontend explicitly regenerates the current logical image and
its reference before entering software outline expansion. A failure while
retrying a deferred GPU submission already disables the GPU and calls
`render_frame` again, which now takes the normal software path.

The main regression risks are accidentally selecting the shortcut with a
software overlay, or reusing stale pixels during fallback. Tests cover both.
The guest framebuffer, CopyBits source data, retained coverage calculations,
palette resolution, tick budgets, input handling and submission scheduling
are unchanged. There is no new invalidation policy. This is a Windows
frontend optimization; the exporter equivalence tests also cover 16- and
32-bit retained surfaces.

## Validation

Baseline: upstream `1444ee3c`, including the merged GPU presenter, Coppet,
snapshot-span optimization, and published `m68k 0.14.0` dependency.

- Library tests: 5,526 passed, 3 ignored.
- Linux desktop tests with `debug-server`: 62 passed.
- Retained-image equivalence tests cover indexed/direct color, scales 2–4,
  changed pixels, reused output buffers, invalid dimensions, and real
  software cursor/debug overlay transitions.

- Native Windows release library, production frontend and replay binaries
  built against explicitly frozen before/after libraries. Both libraries
  were rebuilt from clean package state and checked for non-cached compiler
  artifacts from their respective worktrees.
- A separate, hidden Windows diagnostic compared actual frontend exports
  against the full software retained-image API across seven stages: plain,
  cursor, plain, debug, cursor plus debug, plain, and resized. Every retained
  sample matched. The sequence began after SC2K created retained text.
- That diagnostic injected immediate and deferred GPU submission failures at
  800×600 and 1104×828, plus immediate failure at 500×375. Before failure it
  replaced the old logical buffer with one magenta pixel. In all five cases,
  the actual fallback reconstructed exactly the independently generated
  current software image at the requested dimensions. Each process exited
  successfully. These were hidden registration-screen checks, with silent
  host audio and no injected host input; they establish fallback pixel
  correctness, not visible device-removal behavior or input-to-display
  latency.

## Controlled performance comparison

The native Windows replay uses the same 2,500-frame ordinary-city/Fire script
on each side, in before/after/after/before order. Each invocation starts from
an isolated copy of the same archive. The baseline includes the earlier
reusable compact-cell slice optimization. Neither version displays a window
or submits to the GPU in this replay. No builds, tests, or other validation
processes overlap the timed runs.

The comparison includes logical conversion/reference copying and compact
export, plus Windows process CPU counters across the complete run. Phase
times are elapsed time; process CPU sums all threads and includes startup
and checkpoint overhead. These are distinct from live GUI CPU utilization.

| Work | Before | After | Reduction |
| --- | ---: | ---: | ---: |
| Ordinary city: logical conversion/copy plus compact export, mean per frame | 2.034 ms | 0.788 ms | 61.2% |
| Fire: logical conversion/copy plus compact export, mean per frame | 2.125 ms | 0.799 ms | 62.4% |
| Complete replay: mean total process CPU | 16.516 s | 13.016 s | 21.2% |

Individual complete-run CPU times, in execution order: before 16.796875 s,
after 13.218750 s, after 12.812500 s, before 16.234375 s. Both paired
comparisons improved. Ordinary-city phase statistics use frames 850–999;
Fire statistics use 1300–2499. Removing logical conversion/copy accounts for
roughly 0.72–0.76 ms per frame; eliminating input validation/comparison in
the exporter saves another 0.53–0.57 ms.

All 2,500 non-timing frame records matched across all four runs. At frames
999, 1499, 1999 and 2499, complete 128 MiB guest RAM hashes, CPU register
snapshots, rendered PNGs, and binary compact cells/detail samples matched
exactly. Retained detail was not dropped to achieve the reduction.

These results measure this export change against current master, with the
same recorded power state throughout the paired runs. They do not establish
live total CPU utilization or input-to-display latency. Earlier visible
prototype checks covered an
ordinary city at native/enlarged sizes and native-size Fire, but used a
pre-merge stack; their timings are not measurements of this branch. Future
dirty-image reuse and composition reductions are separate work.
