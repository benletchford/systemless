# Original application reconnaissance

These are bounded startup checks, not application acceptance. No window or game
interaction has been exercised. The binaries are external fixtures and are not
redistributed in this repository.

Source archives were read from the local `systemless-games` corpus. Extract with
`unar -k visible`, then unwrap entry type 2 from the emitted AppleDouble `.rsrc`
file to obtain the raw resource fork. The entry table begins at byte 26, has the
big-endian count at byte 24, and contains 12-byte type/offset/length records.
Do not pass the AppleDouble container directly to `--code` or `--resources`.

| Application input | Bytes | SHA-256 |
| --- | ---: | --- |
| Super Mines Demo raw resource fork (`super-mines.sit`) | 518906 | `0a04894b487ed9e42fd6e67f2b8fe17c050a40b0a4f52ee5a768a1ce524b1ec0` |
| Centaurian FAT data fork (`Centaurian_1.2.1.sit`) | 200976 | `ab5e510dce4ea5ecb1fb80a814d658d9f31f7551fd932018299b9df0def3cd65` |
| Centaurian FAT raw resource fork | 1769685 | `e2864ba05272a3d52a8f9f48ef45b8468403f315c4718518c12f5d52f6106d3f` |

Run `cargo run --manifest-path future/Cargo.toml -- --code RAW_RESOURCE_FORK
--budget 100000` for 68K, or `--pef DATA_FORK --resources RAW_RESOURCE_FORK
--budget 100000` for PPC. The `trace_application` example accepts `code`, the raw
fork path and a maximum instruction count, and prints register/opcode context.

## Observations

- Super Mines initially wrote through zero after reading CurStackBase ($0908),
  eventually faulting at $1000. Its CODE 0 declares 6678 bytes below A5. Publishing
  the start of those globals ($003FE5EA) fixes this specific loader defect. The
  original startup now initializes through $00400000, then passes ReleaseResource,
  InitFonts, InitWindows, FlushEvents and InitMenus with graphics enabled. It stops
  after InitCursor, TEInit, InitDialogs, SysEnvirons and EventAvail at unsupported
  DetachResource ($A992), PC $0100D9F8 (trace index 14460), after
  PBGetCatInfo reports a missing directory, PBDirCreate creates it and
  PBHGetFInfo completes a file lookup and GetNewWindow creates its first
  hidden dBoxProc resource window.
- Centaurian's 68K path originally faulted writing $010000FC at PC $01000270
  (trace index 92039). The target is a JSR operand containing $00000028; startup
  adds A5 to relocate it to $00400028. Classic CODE now permits writes and these
  relocations complete. Startup now also installs Toolbox replacements for
  LoadSeg ($A9F0), UnloadSeg ($A9F1), and ExitToShell ($A9F4). The next stop is
  unsupported GetGDevice ($AA32) at PC $10007426 (trace index 104863), after
  InitFonts/InitWindows/InitMenus/TEInit/InitCursor/InitDialogs/MaxApplZone
  (graphics enabled), HLock,
  StripAddress and executing resource-loaded code from heap RAM. The prior $A204
  request was Sound Driver Control(-4, 2), setting volume to 3. That state update
  now completes. No window interaction or audio playback is proven.
- Centaurian's PPC load rejects the unresolved strong InterfaceLib import
  `SndDisposeChannel`. TEStyleNew/TENew and TextEdit lifetime, TextFont/TextFace/TextMode/TextSize, NewWindow (plainDBox), InitWindows, GetPattern, ForeColor/BackColor, FrameRoundRect, Random, GetMemFragment and the symbol-query imports now resolve, but no PPC
  application instruction executes because remaining strong imports are still
  validated before launch.

The loader now also publishes CurrentA5 ($0904) and the word CurJTOffset ($0934).
CurStackBase denotes the start of application globals; the current separate,
fixed stack reservation remains an explicit runtime limitation. A generated
instruction regression independently reads these globals and writes through
CurStackBase. This proves the launch contract, not original-app compatibility.

Semantics are documented in Apple's [Memory Manager summary](https://dev.os9.ca/techpubs/mac/pdf/Memory/Memory_Manager.pdf)
(page 2-104) and [Inside Macintosh volumes I–III](https://puyoman.net/documentation/Inside_Macintosh.pdf)
(global variables, III-227). ReleaseResource now invalidates loaded resource identity and permits reloads;
its error behavior and changed-resource retention follow Apple’s
[ReleaseResource contract](https://dev.os9.ca/techpubs/mac/MoreToolbox/MoreToolbox-85.html).
InitFonts ($A8FE) now loads a guest system font or allocates the licensed fallback.
InitMenus ($A930) owns and draws an empty bar through shared guest state/wgpu.
InitCursor ($A850) now initializes shared cursor state for the GPU presentation
overlay. InitDialogs ($A97B) now initializes shared dialog globals and parameter
StringHandles. SysEnvirons now reports the shared compatibility profile; common
loading maps/publishes the full fixed heap and MaxApplZone preserves its extent.
Fresh traces pass EventAvail/SetRect and reach DetachResource in Super Mines and GetGDevice in Centaurian. Actual
onscreen cursor behavior, heap-zone resizing, menu/dialog construction, window
activation events and original application interaction remain unfinished.
Dynamic memory loading is verified with the MPW resource-plugin fixture;
Centaurian has not yet executed its own GetMemFragment calls. Full acceptance
still requires original application interactions on both architectures.
