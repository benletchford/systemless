# Graphics kernel qualification

The first parallel candidate is the existing pure indexed 8-bit horizontal
shrink reducer. Its source snapshots are complete before computation, and
tracked destination writes remain in row order afterward. The existing
QuickDraw compatibility algorithm and failure outcomes are unchanged.

Run the opt-in full-operation comparison on an otherwise idle native host:

```sh
cargo test --locked --profile ci-test --lib --features test-support \
  copy_bits::benchmarks::measure_indexed_shrink_full_operation -- --ignored --nocapture
```

The probe compares real `MacMemoryBus` source validation, snapshots, reduction
and tracked writes with the pure reducer alone. It uses four warm-up operations
and 31 samples per case, alternates disjoint source index ranges so every
operation changes destination pixels, and verifies the final destination.
Presentation tracking is tested both disabled and enabled at 2× retained scale.
The comparison uses one, two and four total participants, including the owner,
and records the first operation separately from warmed samples. It forces pool
dispatch even for small cases to reveal the crossover; ordinary execution keeps
small jobs serial.

Initial Apple M1 screening in the optimized `ci-test` profile produced these
median times. This is not a release-mode qualification or a gameplay profile;
other build activity was present on the host.

| Source → destination | Presentation | Full operation | Pure reduction |
| --- | --- | ---: | ---: |
| 64×64 → 32×64 | Offscreen | 10.7 µs | 8.3 µs |
| 64×64 → 32×64 | Tracked screen | 38.3 µs | 8.2 µs |
| 640×480 → 320×480 | Offscreen | 446.5 µs | 424.2 µs |
| 640×480 → 320×480 | Tracked screen | 2.50 ms | 0.435 ms |
| 2048×2048 → 1024×2048 | Offscreen | 5.97 ms | 5.75 ms |
| 2048×2048 → 1024×2048 | Tracked screen | 34.64 ms | 5.81 ms |

Large offscreen reduction is a plausible candidate because it dominates that
operation. Screen tracking dominates the visible case: even eliminating the
measured reduction entirely would not meet a 20% full-operation target there.
A native persistent pool now computes independent row ranges from immutable
owned snapshots. The owner participates in computation and commits through the
same tracked-memory APIs in original order. Each worker has one bounded request
slot and one bounded result slot; workers are reused and joined when the owner
exits. A disconnected worker causes a serial retry of pure computation before
any destination write. Partial guest writes are never replayed.

The pool is experimental: `SYSTEMLESS_GRAPHICS_PARTICIPANTS=2` or `4` selects it.
The default is `1` (serial), and Wasm remains serial. The provisional threshold
is 256 KiB of captured source rows. It is not yet a qualified default.

A subsequent same-build `ci-test` comparison, with no other compiler or browser
probe active, includes snapshots, dispatch, synchronization, output assembly
and tracked commit in these full-operation medians:

| Source size | Presentation | 1 participant | 2 participants | 4 participants |
| --- | --- | ---: | ---: | ---: |
| 64×64 | Offscreen | 10.1 µs | 13.3 µs | 16.0 µs |
| 64×64 | Tracked screen | 35.6 µs | 41.1 µs | 43.3 µs |
| 640×480 | Offscreen | 425.1 µs | 265.2 µs | 179.8 µs |
| 640×480 | Tracked screen | 2.38 ms | 2.21 ms | 2.13 ms |
| 2048×2048 | Offscreen | 5.71 ms | 3.16 ms | 1.75 ms |
| 2048×2048 | Tracked screen | 32.31 ms | 29.77 ms | 28.38 ms |

The initial four-participant gain is 69% for the large offscreen operation and
12% for the tracked screen. Small-job overhead confirms that dispatch needs a
threshold. These are screening results, not final release qualification.
Exact differential tests cover clipped and uneven rows, rounded tails, guard
and identity-map behavior, repeated pool reuse, worker disconnection, aliased
source/destination rows, read failure and partial ordered writes. All guest
memory accesses are checked to remain on the owner.

Intermediate-size crossover measurements, repeated release measurements and
gameplay regression checks remain outstanding. Native execution stays serial
unless the experimental environment setting is supplied.
