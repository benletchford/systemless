# Writable volume backing

The optional `--volume PATH` application-launch option loads a writable catalog
before the guest runs and checkpoints changes at host execution boundaries.
Without it, files remain process-local. Both CODE and PEF applications use the
same codec, File Manager and backing implementation.

```sh
cargo run --manifest-path future/Cargo.toml -- --code application.rsrc --window --volume saves.slv
cargo run --manifest-path future/Cargo.toml -- --pef application.pef --resources application.rsrc --volume saves.slv --budget 100000
```

The parent directory must exist. A missing image starts a fresh writable catalog;
an existing invalid image fails before execution and is never silently replaced.
The current application is loaded normally as protected node 16. Its executable
forks and loader-owned resource reference are preserved, not imported from or
copied into the volume. Mutable files, directories, IDs, raw Macintosh names,
Finder bytes, creation/modification dates and both forks survive restart.
Open access paths, marks, WD references, heap/CPU/UI state and resource handles do
not. Different CPU backends may reopen the same saved volume sequentially.

`files::Files::{export_volume,restore_volume}` are bounded pure byte operations.
Restore stages and validates the entire image before replacing a fresh writable
catalog; loader-owned application access paths stay intact. Restore rejects
reserved/duplicate IDs, invalid names, duplicate case-insensitive siblings,
missing/non-directory parents, directory cycles, invalid counters, excessive
counts/lengths, trailing bytes, unknown format fields and checksum failures.
Fork storage retains the runtime's aggregate 64 MiB limit and at most 1,024 mutable
nodes. The image limit is 64 MiB plus 128 KiB of catalog overhead. Names retain
the File Manager's existing ASCII case-fold policy; full Macintosh collation is
unfinished. Exhausted node IDs report an error without wrapping.

`volume::Volume` owns the host path and an exclusive OS lock on a stable
`PATH.lock` sidecar, which is intentionally retained after closing. The OS
releases its advisory lock when the handle/process closes; a leftover sidecar
does not itself indicate a live writer. The image is bounded-read and decoded
under that lock. Updates write a new sibling with create-new semantics, sync its
contents, atomically rename it over the image and, on Unix, sync the parent
directory. Failed writes/replacements leave the old complete image available;
failed temporary outputs are removed when possible. A parent-directory sync
failure can be reported after replacement has already installed a complete new
image. Existing volume symlinks
and non-regular files are rejected. Other programs must cooperate with locking.

A host-only revision counter tracks successful create/delete/metadata/write/EOF,
directory creation and resource-file initialization. Read/open/close, WD state
and failed mutations do not dirty the saved catalog. Unchanged boundaries avoid
serialization and host writes. Open writable fork contents are included, so the
checkpoint is independent of whether an application closes every access path.
The windowed frontend checkpoints after guest execution/presentation attempts
and on host close. Headless execution checkpoints after advance returns, including
budget/fault boundaries, before reporting the outcome. Host I/O failures stop the
host and report the error; they are not yet mapped back to the guest's already
completed synchronous File Manager call. PBFlushFile is not a per-service host
fsync guarantee. Abrupt termination can lose work since the last completed host
checkpoint; concurrent mounting, journaling and crash recovery of in-flight guest
operations remain unfinished. Native-window checkpoint wiring is compiled; the
cross-launch behavioral evidence below uses the offscreen/CLI hosts.

## Version 1 wire format

All integers are unsigned big-endian. CRC-32/IEEE covers every byte except the
final checksum; it detects accidental damage and is not an authentication format.

| Header offset | Bytes | Meaning |
| --- | ---: | --- |
| 0 | 8 | ASCII `SLVOLUME` |
| 8 | 4 | Version, exactly 1 |
| 12 | 4 | Next mutable node ID |
| 16 | 4 | Directory count |
| 20 | 4 | File count |
| 24 | 4 | Payload byte length |
| 28 | 4 | Reserved, zero |

Directories precede files, each group sorted by node ID. A directory contains
ID, parent ID, creation date and a one-byte-length raw name. A file contains ID,
parent ID, creation date, modification date, 16 Finder bytes, data-fork length,
resource-fork length, the raw name and both fork byte strings in that order.
Names have 1–31 bytes and contain neither NUL nor colon. Root ID 2 and application
ID 16 are reserved; mutable IDs start at 17 and are less than the next ID.
The payload is followed by one four-byte checksum. This is the independent
runtime's volume format, not an HFS disk image or a legacy save-state format.

## Behavioral evidence

The public-header `persistence.c` guest creates `Saves:Game`, Finder type SAVE,
with data `PERSIST` and resource bytes `RSRC`. A fresh PPC guest reopens a volume
written by 68K, validates metadata and reads both forks; another fresh 68K guest
and separate CLI processes repeat the read without changing the saved bytes.
Codec/host tests exercise transactional malformed-image failures, topology,
checksum, bounds, stable IDs/metadata/forks, writer exclusion, failed atomic
replacement/retry and exhausted IDs. Mutation tests require checkpoints for
metadata, EOF, resource initialization and deletion while failed deletes and
access-path changes leave the image unchanged.

The bounded original-app probe accepts `SYSTEMLESS_PROBE_VOLUME=PATH` and uses
the same backing at advance boundaries. Original Super Mines cross-process
save/reload evidence is recorded in [original applications](original-applications.md).
No application binary, volume image or screenshot is committed.
