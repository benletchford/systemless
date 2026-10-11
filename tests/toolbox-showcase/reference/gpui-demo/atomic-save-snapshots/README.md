# Atomic save snapshots 

The store stages one binary file: SLSAVE01 magic, three big-endian u64 lengths,
metadata JSON, raw data fork and raw resource fork. After writing/syncing the
complete file it renames it to snapshot.bin and syncs the containing directory.
Published snapshots are authoritative; legacy metadata.json/data.fork/resource.fork
remain readable when no snapshot exists. Staging files are ignored by discovery.

Eleven store tests pass, including original session restore cases, legacy
migration, rejected incomplete/trailing/overflow lengths and interruption before
publication. Abrupt subprocess exits immediately before rename and after
successful publication recover complete old/new forks and metadata respectively.
The actual worker exit regression passes12 cases across four CPU/depth modes and
Shutdown/disconnect/guest Quit (88.42s), including independent guest readback.

These tests cover process interruption at chosen publication boundaries. They
do not qualify sudden power loss, directory-tree creation/deletion durability,
filesystem failure injection or macOS full-storage barriers. The complete GPUI
production gate remains open. Source comments added after the worker build do
not change behavior; final evidence will record the source checkpoint explicitly.
