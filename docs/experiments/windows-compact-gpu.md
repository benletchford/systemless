# Windows compact GPU prototype: displayed validation, 2026-09-16

The local opt-in D3D11 prototype reduces scaled CPU work while preserving the single-pass coverage result. It is not ready for a production PR: scaled presentation pacing still needs work, and the controlled software comparison predates merged #1997. The default renderer is unchanged.

## Controlled pair

Same instrumented executable, GPU then software; 15-second measurement windows; AC, 99%, Balanced, battery saver off. No concurrent compiler work. Fresh cities are not instruction-identical replays. CPU figures are main-thread elapsed work, not GPU completion or display latency.

| Stage | Software median / p95 ms | GPU median / p95 ms | Accepted GPU presents/s |
| --- | ---: | ---: | ---: |
| new-city-native | 6.196 / 7.085 | 6.175 / 7.211 | 60.1 |
| new-city-scaled | 22.284 / 23.742 | 6.015 / 7.038 | 49.8 |
| city-scaled | 22.852 / 31.103 | 6.746 / 16.679 | 51.7 |
| city-native | 7.072 / 16.540 | 6.758 / 16.601 | 59.9 |

The software branch is based on c72bc8d, before #1997; these numbers are not a comparison with current master. GPU compact preparation now costs about 1.48 ms in the city (previous prototype about 4.64 ms). Scaled presentation still accepts fewer frames than update attempts, despite reduced CPU work. Accepted presents are submissions, not measured display refreshes.

## Correctness and limits

The independent CPU coverage oracle matched all 36 sampled readbacks, totaling 22,169,408 pixels, across sizes 1100x760, 1104x828, 1240x930, 400x300, 800x600. The GUI run covered registration, New City, newspaper, city, resize and minimize/restore. All 32 presentation tests pass on native Windows.

- Scaled GPU presentation accepts about 50–52 frames/s despite approximately 60 update attempts/s.
- DO_NOT_WAIT/zero-time readiness polling needs pacing work before production use.
- Five menu trials per variant include outliers; no demonstrated end-to-end latency improvement.
- Fire selection script did not trigger a visible fire or National Guard dialog; those captures are ordinary city views. Fire is not validated by this run.
- No controlled integrated comparison against merged #1997 yet.
- Device loss, Windows versions other than this host, and physical input-to-scanout latency remain untested.

Raw timing summaries, per-trial menu JSON, power records, screenshots and binary/source hashes are stored alongside this report. `gpu-compact-v3-results.json` collects the results. The branch is `experiment/windows-compact-gpu`, local worktree `windows-compact-gpu`; no GPU PR has been opened. The experiment is preserved separately from the next font comparison work.

## Running the prototype

Build the Windows GUI with `cargo build --release --features gui --target x86_64-pc-windows-gnu`. Set `SYSTEMLESS_D3D11=1` when launching. Unset it to use the existing software renderer. Set `SYSTEMLESS_GPU_VERIFY=1` only for correctness checks; synchronous readback changes timing and must be disabled for performance measurements. `SYSTEMLESS_GPU_CAPTURE_DIR` optionally saves verified output. Native hardware cursors are independent of this switch.

This branch deliberately preserves the tested c72bc8d-based experiment. Before proposing a production PR, integrate current master and rerun the controls with #1997 included. The shader uses original retained samples and integer coverage weights; the transport sends one color per ordinary logical pixel plus high-resolution samples for text cells, avoiding a full 4x ARGB upload. No font design or guest execution optimization is part of this experiment.
