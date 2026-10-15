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
  InitFonts, InitWindows, FlushEvents and InitMenus with graphics enabled. It proceeds
  through InitCursor, TEInit, InitDialogs, SysEnvirons and EventAvail, then enters
  event/mouse polling through GetMouse and PtInRect after
  PBGetCatInfo reports a missing directory, PBDirCreate creates it and
  PBHGetFInfo completes a file lookup and GetNewWindow creates its first
  hidden dBoxProc resource window and DetachResource transfers loaded
  resource ownership, NewControl creates its button-family controls,
  SizeWindow resizes the hidden window, SetWRefCon stores application data,
  MoveWindow positions it, ShowWindow reveals it and InvalRect requests redraw.
  A 100,000-instruction trace completes without an unsupported service.
  A bounded press/release at global (256,171) passes FindWindow, FindControl
  and StillDown, hides/disposes the demonstration splash, restores its saved
  nil port and reaches unsupported PBHOpen ($A200).
  SystemTask preserves the empty periodic service state, PenSize updates live
  GrafPort dimensions and RectInRgn checks content intersections.
  GetPicture loads PICT 202, PortSize sizes its offscreen GrafPort and MovePortTo
  sets the bitmap mapping before drawing. DrawPicture now decodes that v1
  PackBitsRect/srcOr resource and executes through wgpu. CopyBits transfers
  the offscreen images. LineTo now draws through the shared GPU pen path
  and EraseRect clears its requested area before startup reaches a later
  picture containing default-highlight metadata and an 8-bit indexed PixMap.
  PICT 184 now completes table decoding, packed rows, mask and GPU monochrome
  conversion before the application requests a text box.
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
Fresh traces pass EventAvail/SetRect, bold TETextBox, GetMouse and PtInRect in Super Mines; a press/release dismisses the splash and reaches PBHOpen. Centaurian reaches GetGDevice. Actual
onscreen cursor behavior, heap-zone resizing, menu/dialog construction, window
custom definitions and original application interaction remain unfinished.
Dynamic memory loading is verified with the MPW resource-plugin fixture;
Centaurian has not yet executed its own GetMemFragment calls. Full acceptance
still requires original application interactions on both architectures.

Super Mines TextBox calls now complete with the selected family-3, size-9 bold
strike. Layout and clipping use the shared TextEdit/GPU route, with a synthesized
one-pixel bold source. A bounded press/release now dismisses that original splash;
gameplay and complete lifecycle acceptance remain pending. Native styled-text
pixel comparison has not passed. Legacy Systemless
comparison alone is permitted by the user; native System 7 execution is optional.

A reproducible bounded click probe captures PPM images before and after dispatch:

```sh
cargo run --manifest-path future/Cargo.toml --example probe_mouse -- CODE_RESOURCE_FORK /tmp/mines-probe 256 171
```

This diagnostic loads a 68K CODE fork, runs 30,000 startup boundaries, injects
mouse-down, then permits up to 30,000 more boundaries before mouse-up and another
30,000. It propagates service faults; screenshots are evidence, not automatic
acceptance. No original application/resource bytes are added to the repository.

The current probe at (256,171) records mouse-down Budget, then mouse-up failing
at PBHOpen ($A200), after HideWindow, control/resource disposal, DisposeWindow
and SetPort(nil). Before/after screenshots show the demonstration splash replaced
by the desktop. This proves one original 68K splash-dismissal interaction, not
menus, gameplay, preferences or a complete application lifecycle. Next: shared
hierarchical data-fork opening and its real access/error behavior, then resume
this same application transition. The full rebuild goal remains active.
