# Graphics kernel qualification

The first parallel candidate is the existing pure indexed 8-bit horizontal
shrink reducer. Its source snapshots are complete before computation, and
tracked destination writes remain in row order afterward. The existing
QuickDraw compatibility algorithm and failure outcomes are unchanged.

Run the opt-in full-operation baseline on an otherwise idle native host:

```sh
cargo test --locked --profile ci-test --lib --features test-support \
  copy_bits::benchmarks::measure_indexed_shrink_full_operation -- --ignored --nocapture
```

The probe compares real `MacMemoryBus` source validation, snapshots, reduction
and tracked writes with the pure reducer alone. It uses four warm-up operations
and 31 samples per case, alternates disjoint source index ranges so every
operation changes destination pixels, and verifies the final destination.
Presentation tracking is tested both disabled and enabled at 2× retained scale.

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
Persistent-pool measurements with one, two and four total participants, exact
differential tests, a measured dispatch threshold, release validation and
gameplay regression checks remain outstanding. Native execution remains serial.
