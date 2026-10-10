# Control state and completed paint freshness

Both CPU adapters now stamp exact guest visibility/highlight/value/range and
CDEF-handle bytes, plus the complete used Pascal title, alongside completed
control pixels. Unused title padding is excluded. A direct application write
therefore cannot make a new GPUI label or value claim old guest painter ink.
Actual redraw recovery compares the surface separately from the recipe so the
original background remains recoverable after a legitimate state change.

Seven shared paint tests pass; the actual PPC checkbox draw/mutation/redraw test
passes (0.08s), and classic push-button/checkbox/radio font draws with raw value
mutation rejection pass (0.18s). The shared binary predates the added PPC test;
production paint/adapters are unchanged between these checkpoints. Source hashes
pin production code. These tests establish native guest draw/state ownership,
not screenshot fidelity, physical host interaction or the complete release gate.

The current default GPUI application compile check also passes (25.04s).

The later live-control fixture regression passes all four explicit CPU/depth
modes (6.36s), requiring fresh standard button/checkbox backgrounds in actual
frontend snapshots and preserving guest held checkbox/value/scrollbar behavior.
Evidence is `four-mode-snapshots.log`; `snapshot-source-hashes.json` pins this
later source checkpoint. Snapshot metadata is not composed visual evidence.

The no-default-features public library compile check passes (11.88s).
