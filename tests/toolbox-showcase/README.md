# Toolbox showcase fixture

This directory contains the source and reproducible build for a classic
Macintosh fat application. The same `showcase.c` is compiled into a 68K
`CODE` slice and a native PowerPC PEF slice. The PEF remains in the data fork;
the 68K code, `cfrg`, menus, windows, dialogs, and other resources share the
resource fork. Both forks are committed in `toolbox-showcase.sit`.

The public coverage is tracked by issues #1078, #1081, #1264–#1270,
#1338–#1339, #1344, and #1368.

The application deliberately uses ordinary Toolbox APIs rather than a private
test protocol. Its Pages menu selects sixteen interactive views:

1. Graphics exercises patterns, clipping, indexed color, lines, shapes, and
   text.
2. Controls exercises a push button, checkbox, and scroll bar. Successful
   actions appear as checkmarks in the State menu.
3. Windows creates three visibly overlapping document windows; the scripted
   contract activates, moves, resizes, hit-tests, and closes them while
   checking Window Manager order and repaint state. The stacked inspector
   writes an application-defined `WStateData.stdState` rectangle, allowing
   zoom and restore to exercise guest-owned bounds on both CPU slices.
4. Drawing & 3D Bevels exercises polygons, arcs, regions, pictures, icons,
   fonts, styles, and metrics. The PowerPC slice also builds and submits a lit
   QuickDraw 3D TriMesh through a view, camera, renderer, and draw context,
   and keeps its rendered scene visible. The 68K slice displays a labelled
   QuickDraw bevel fallback.
5. Game Preferences presents a game-style configuration panel with audio
   checkboxes, difficulty and renderer radio groups, a volume scroll bar, and
   action buttons. Its settings stay synchronized with hierarchical menus.
6. Dialogs & Alerts exercises resource-backed modal dialogs, controls,
   editable text, and a system alert. Options → Modeless Dialog opens a
   separate resource-backed standard dialog whose activation and close
   lifecycle can be exercised while the showcase window keeps running.
7. TextEdit exercises an interactive multiline `TERec` buffer, character
   insertion and selection, paragraph alignment (`teJustLeft`, `teJustCenter`,
   `teJustRight`), clipboard scrap operations (`TECut`, `TECopy`, `TEPaste`),
   transient wrapped text formatting (`TETextBox`), and live record metrics
   inspection.
8. Palettes activates a resource-backed mixed-usage palette, draws through
   `PmForeColor` and `PmBackColor`, translates an unrelated indexed PICT
   through a canonical offscreen GWorld into the active screen palette,
   preserves positional indexes copied from a same-identity device ColorTable
   whose RGB entries are transiently black, verifies that `RGBForeColor` uses
   the screen GDevice inverse table when the logical and hardware CLUTs differ,
   and animates explicit CLUT entries without redrawing their indexed pixels.
   Both slices record and replay the PICT through the same visible path.
9. Lists & Inventory creates a default text list with a vertical scroll bar,
   selects and inspects a cell, mutates its contents, scrolls and resizes the
   list, and toggles List Manager activation.
10. Sound & Channels creates a sampled channel, plays a format-1 `snd `
    resource, verifies SysBeep PCM, queues volume and callback commands,
    flushes and quiets the channel immediately, observes completion, and
    disposes the channel.
11. Styled Text & Fonts creates a live `TEStyleNew` record, applies multiple
    `TESetStyle` runs, inspects mixed and continuous attributes with
    `TEContinuousStyle`, resolves Geneva and Monaco through `GetFNum`/`RealFont`,
    and compares `CharWidth`, `TextWidth`, and `MeasureText` results from the
    same Font Manager state that renders the record. Clicking the styled field
    focuses it through `TEActivate`/`TEClick`; typing and deletion use `TEKey`,
    idle caret updates use `TEIdle`, and suspend/resume follows the guest
    activation events. Leaving the page clears that focus.
12. Standard File exercises modern and legacy Open and Save entry points,
    filters the Open list to `TEXT`, navigates into the fixture folder,
    accepts a returned `FSSpec`, edits a Save name, and cancels both legacy
    paths while checking `StandardFileReply` and `SFReply` fields. A new-name
    Save writes a known data-fork payload, closes and reopens it, checks every
    byte, then retains it for desktop save/reload checks. The main window’s app-owned refCon publishes the
    completion status for tests. This does not test persistence across a host
    restart. Confirmed replacement truncates the existing data fork and checks
    the reopened length and bytes too, leaving the replaced file in that guest
    session. Tests load independent sessions from the immutable source archive.
13. Resource Browser enumerates named `DATA` records with
    `Count1Resources`, `Get1IndResource`, `GetResInfo`, `GetResAttrs`, and
    `GetResourceSizeOnDisk`, then demonstrates deferred `GetNamedResource`/
    `LoadResource`, `ReleaseResource`, and reload of the same map reference.
14. Sprites, Masks & Scrolling builds an indexed offscreen scene, transfers a
    sprite through `CopyMask`, transfers a second frame through `CopyDeepMask`
    and a `BitMapToRegion` clip, samples pixels with `SetCPixel`/`GetCPixel`,
    and scrolls the existing raster with `ScrollRect`.
15. Events & Cursors records raw `EventRecord` mouse/key/modifier fields,
    samples `GetMouse`, `Button`, `StillDown`, `WaitMouseUp`, and `GetKeys`,
    peeks and consumes a posted key event with `EventAvail`, `OSEventAvail`,
    and `GetOSEvent`, and switches standard cursors with `GetCursor`,
    `SetCursor`, `InitCursor`, `HideCursor`, and `ShowCursor`.
16. Popup & Dropdown Lists combines a resource-backed `CNTL`/`MENU` popup with
    a programmatic `NewMenu`/`NewControl` popup. It exercises standard popup
    CDEF tracking, disabled and separator rows, a long label, the
    `popupFixedWidth` and `popupUseWFont` variations, a genuinely scrollable
    55-item menu, synchronized control values/menu marks, and save-under
    restoration of the closed controls.

The menus also cover checkmarks, keyboard equivalents, three levels of
hierarchical game options, and switching menu-bar selections while a menu is
already being tracked.

These calls follow the contracts in *Inside Macintosh: Macintosh Toolbox
Essentials* (1992), Event Manager pp. 2-50–2-71, Menu Manager pp. 3-48–3-65,
Window Manager pp. 4-63–4-93, Control Manager pp. 5-78–5-96, and Dialog
Manager pp. 6-43–6-84. TextEdit follows *Inside Macintosh: Text* (1993),
pp. 2-63–2-114. The drawing surface follows *Inside Macintosh: Imaging
With QuickDraw* (1994), pp. 3-38, 3-55–3-95, and 4-68. Palette activation,
usage categories, indexed drawing, and animation follow *Inside Macintosh,
Volume VI* (1991), pp. 20-8–20-22. Lists follow *Inside Macintosh: More
Macintosh Toolbox* (1993), pp. 4-26–4-42 and 4-65–4-95.
Raw input follows *Inside Macintosh: Macintosh Toolbox Essentials* (1992),
pp. 2-18–2-19, 2-50–2-71, and 2-97–2-110; cursor handling follows *Inside
Macintosh: Imaging With QuickDraw* (1994), pp. 8-22–8-29.
Sound follows *Inside Macintosh: Sound* (1994), pp. 2-19–2-29, 2-92–2-101,
2-121–2-123, and 2-151–2-152. The styled TextEdit and Font Manager checks
follow *Inside Macintosh: Text* (1993), pp. 2-78, 2-98–2-102, 3-81–3-82,
and 4-52–4-53. Standard File follows *Inside Macintosh: Files* (1992),
pp. 3-42–3-54. Resource enumeration, metadata, deferred loading, handle
release, and reload follow *Inside Macintosh: More Macintosh Toolbox* (1993),
pp. 1-75–1-82, and the Resource Manager overview and lifecycle contracts are
cross-checked against *Inside Macintosh Volume I* (1985), pp. I-118–I-125.
Sprite masking, offscreen worlds, pixel sampling, regions, and scrolling
follow *Inside Macintosh: Imaging With QuickDraw* (1994), pp. 2-20–2-24,
2-43–2-50, 3-119–3-122, and 6-22–6-46.
Popup controls and menus follow *Inside Macintosh: Macintosh Toolbox
Essentials* (1992), Menu Manager pp. 3-31–3-34 and Control Manager
pp. 5-25–5-27, cross-checked against *Inside Macintosh, Volume VI* (1991),
pp. 3-16–3-19 for popup private data, menu IDs, selected-item values, and
the standard `TrackControl` contract.

## Rebuild and verify

Docker is the only build prerequisite. The image pins the `mps` source commit,
checks the MPW image checksum before installation, and pins `macresources`.
The fixture-local, non-publishable Rust packer pins the same released
`stuffit` crate as the runtime.

```sh
./tests/toolbox-showcase/build.sh
./tests/toolbox-showcase/build.sh --verify
```

`--verify` rebuilds the application and fails unless the resulting StuffIt
archive is byte-for-byte identical to the committed archive. Maintainers use
`--update` only when intentionally changing the fixture source or toolchain.
All intermediates remain under the ignored `build/` directory.

## Systemless interaction contract

The integration test launches the committed archive twice: the default launch
selects the 68K `CODE` slice, and the second launch selects the native PEF with
`SYSTEMLESS_PREFER_POWERPC=1`. Both runs use the same semantic assertions and
exact Systemless framebuffer references while performing the same sequence:

1. Confirm Graphics (Pages menu 129, item 1), the initial menu state, and one
   main window.
2. Choose Controls (item 2), then click the button, checkbox, and right scroll
   arrow. State menu 130 items 1–3 must become checked.
3. Choose Windows (item 3). Two overlapping document windows must appear and
   State item 4 must become checked. The semantic Window Manager checkpoint
   records the front-to-back stack, active window, port/structure geometry,
   visible regions, and empty update regions after repaint. Probe the overlap,
   activate the auxiliary document, drag its title bar, resize it with the
   grow box, and then activate the inspector through an exposed region. Close
   the inspector and the promoted auxiliary document in turn; each close must
   promote the predecessor and repaint the newly exposed content. The
   deterministic Systemless frames are `03-windows.png`,
   `03-windows-aux-activated.png`, `03-windows-moved.png`,
   `03-windows-resized.png`, `03-windows-hit-test.png`,
   `03-windows-promoted.png`, and `03-windows-main-promoted.png`.
4. Choose Drawing & 3D Bevels (item 4), verify representative QuickDraw
   output, require a completed native PowerPC frame, and compare its visible
   geometry silhouette and interior lighting with the SheepShaver capture.
5. Choose Game Preferences (item 5), change its controls, verify the matching
   hierarchical-menu checkmarks, then change menu items and verify the panel.
6. Open File → Game Options and capture the nested submenu while it is live.
7. Choose Dialogs & Alerts (item 6), open the resource-backed modal dialog,
   modify it, and confirm it with OK.
8. Invoke the system alert, capturing its live modal state on implementations
   that block for a response, then dismiss it or record its return.
9. Verify the final dialog status after both modal sessions.
10. Choose TextEdit (item 7), drag to select text, reset the selection, copy,
    cut, paste, type over the selection, reset the edited buffer, and center
    the paragraph. Capture each state and assert the buffer and private scrap.
11. Activate the Palettes page (item 8), verify the indexed PICT → GWorld → screen
    transfer retains distinct colors across unrelated CTables, verify a
    same-device transfer retains its positional indexes through a transient
    black device ColorTable, verify `RGBForeColor` resolves through the
    indexed screen GDevice inverse table when the logical and hardware CLUTs
    differ, and capture the initial tolerant and animated-explicit color
    environment.
12. Click Animate Palette and capture the same indexed pixels recolored by
    `AnimateEntry` without repainting the swatches.
13. Open File, drag across the menu bar to Pages, and capture Graphics
    highlighted before releasing.
14. Confirm the release selected Graphics, restored the default color
    environment, and disposed the auxiliary window.
15. Activate Lists & Inventory (item 9), inspect a selected cell through
    `LGetSelect`/`LGetCell`, and capture the initial and selected list states.
16. Update the selected row with `LSetCell`, scroll with `LScroll`, and resize
    with `LSize`, capturing each resulting list state.
17. Toggle the list inactive and active with `LActivate`, capturing both
    activation states.
18. Activate Sound & Channels (item 10), verify SysBeep output, then exercise
    sustained `SndPlay` playback. Prove queued volume waits, flush retains the
    current sample, quiet stops it, and the flushed callback never arrives.
    Compare complete waveforms at full, 75% and 50% volume with both native
    emulators, verify the callback checkmark, then dispose the channel.
19. Activate Styled Text & Fonts (item 11), verify the rendered multistyled
    TextEdit and its Font Manager/measurement readouts, and capture the page.
20. Activate Standard File (item 12). Capture the page, open the modern
    filtered dialog, enter `Standard File Fixtures`, and accept its `TEXT`
    document with Return. Then cancel `SFGetFile`, accept `StandardPutFile`
    after replacing its default name, and cancel `SFPutFile`. The integration
    checkpoints are `20-standard-file-page.png`,
    `21-standard-file-open.png`, and `22-standard-file-complete.png`; the
    semantic assertions cover returned `FSSpec`/`SFReply` fields and the
    `FSpCreate`/`FSpDelete` round trip.
21. Activate Resource Browser (item 13), capture the map-only enumeration of
    `DATA` 201–203 and the nine `MENU`/one `WIND` counts, refresh the map, load the
    named `DATA` 203 record, release its handle, and load it again. Capture
    the enumeration, loaded, and released lifecycle states.
22. Activate Sprites, Masks & Scrolling (item 14), and verify the two masked
    sprites, pixel probe, region status, and initial scene.
23. Click Animate Sprite and capture the changed source frame after the
    offscreen scene is rebuilt.
24. Click Scroll Scene and verify the sprite moves left by 24 pixels, the
    right-hand strip is repainted, and `ScrollRect` reports the exposed update
    region. Reset the scene and require its raster and validation readouts to
    match the first visit, capturing `28-sprites-reset.png`. The integration checkpoints are `26-sprites.png`,
    `27-sprites-animated.png`, and `28-sprites-scrolled.png`.
25. Reopen Windows, select the main document from the auxiliary window, and
    choose Events & Cursors (item 15) to record `activateEvt` and `updateEvt`
    lifecycle transitions.
26. Hold the page's queue probe button down to record live mouse state,
    confirm `WaitMouseUp` remains true, then post a key event and verify that
    `EventAvail` and `OSEventAvail` peek without consuming before
    `GetOSEvent` removes it.
27. Hold Shift while typing a printable key and verify `shiftKey` in the
    `EventRecord` plus a nonzero `GetKeys` map. Select cross and watch cursors,
    hide and show the cursor, restore the arrow cursor, and capture the five
    event/cursor frames at checkpoints 29–33.
28. Activate Popup & Dropdown Lists (item 16), verify the resource-backed and
    programmatic popup menus, their initial marks, and the closed controls.
    Capture `34-popup-lists.png`.
29. Open the resource popup, move across its separator and disabled row, then
    release without selecting. Verify the original value/mark and restored
    closed control; reopen it and select the long enabled row.
30. Open the fixed-width programmatic popup, release on its disabled row, then
    hold over the down indicator until the long menu reveals item 36, select
    `Deep Field Archive`, and capture `36-popup-lists-scrolled.png`. Reopen
    the popup, scroll back to `Night Operations`, and capture the restored
    controls in `37-popup-lists-selected.png`.

For a manual launch from the public repository:

```sh
cargo run --release -- tests/toolbox-showcase/toolbox-showcase.sit
SYSTEMLESS_PREFER_POWERPC=1 cargo run --release -- tests/toolbox-showcase/toolbox-showcase.sit
```

## Classic-Mac oracle runs

The current [coverage report](COVERAGE.md) maps every Pages-menu item to
Systemless assertions and native evidence. The complete portable native
sequence is [`oracle/overview.json`](oracle/overview.json): 59 checkpoints on
each emulator, including the remaining controls, dialogs, files, resources,
sprites, events and configured SysBeep alert. Six focused scenarios extend
that overview for Windows, Drawing, TextEdit, Lists, Popup and sampled audio.
CI verifies the fixture/scenario/capture identities and complete page inventory
with [`oracle/verify_coverage.py`](oracle/verify_coverage.py). A fresh overview
is checked with [`oracle/verify_overview.py`](oracle/verify_overview.py), which
compares reviewed outcome regions, independent state transitions and captured
audio. Identity checks alone do not claim a fresh native run.


Expand the same `toolbox-showcase.sit` on a shared HFS volume. Launch **Toolbox
Showcase** in BasiliskII for the 68K slice and in SheepShaver for the native
PowerPC slice, then replay the committed overview and focused scenarios. Use an 800×600
8-bit display in BasiliskII and an 800×600 32-bit direct-color display in
SheepShaver for captures matching this gallery. The Pages, State, and nested
menu checkmarks, window count, control values, modal sessions, visible drawing,
and final page provide the comparison points between runs. For the event
page, compare EventRecord kind/coordinates/modifiers, the peek-versus-take
queue results, and cursor shape/hotspot/visibility in addition to the
rendered layout. The event/cursor rows contain deterministic captures from
both classic emulator runs, including the hidden-cursor state.

The Resource Browser checkpoint shows the same three named `DATA` records (IDs
201, 202, and 203), their stable byte sizes, clean attributes, and the
transitions `enumerated → loaded → released → reloaded`. The named-load step
leaves only `DATA` 203 resident; releasing it returns that row to an empty
handle before the reload.

The committed Standard File oracle frames show the page before interaction,
the filtered Open dialog with `Standard File Fixtures` selected, and the final
page after Open, legacy Open cancellation, editable Save acceptance, and
legacy Save cancellation. On the classic systems, the accepted Open result
names `Text Document` with type `TEXT`, the Save name is the single edited
name, and canceled calls leave `sfGood`/`good` false. Standard File window
placement and font rasterization remain presentation variance between system
software versions.

The Sprites checkpoints show the shared offscreen scene after `CopyMask` and
`CopyDeepMask`, the animated source frame, and the 24-pixel `ScrollRect`
movement with its exposed update strip. The first scene must already match
Reset Scene, with successful pixel and region readouts. All offscreen images
are locked while drawing, copying or scrolling: an unlocked native GWorld
base address is a handle rather than the pointer these operations require
(*Imaging With QuickDraw*, 1994, pp. 6-32–6-33). The full replay caught a damaged
first-visit mask and failed readouts that a later rebuild had concealed.
Classic-Mac rasterization and indexed
versus direct-color quantization remain presentation variance.

The Popup & Dropdown Lists checkpoints exercise live standard popup tracking
on both classic emulators. The fixture passes `Pointer(-1)` to `TrackControl`
so the popup CDEF performs its action; `nil` only highlights the control.
The resource popup rejects separator and disabled rows, then selects the long
enabled label. The programmatic popup rejects its disabled row, scrolls a
55-item menu to reveal and select item 36 (`Deep Field Archive`), then scrolls
back to select item 4 (`Night Operations`). The extra
`36-popup-lists-deep-selected.png` checkpoint records the accepted item 36
before reopening. Both final controls retain their values and repaint after
tracking. The longer menu exceeds the viewport even with the classic system's
smaller window font; the earlier 39-item menu could fit without scrolling.
Classic fonts and popup CDEF chrome remain presentation variance.

The Drawing replay is [`oracle/drawing.json`](oracle/drawing.json), with
capture identities in [`oracle/drawing-capture.json`](oracle/drawing-capture.json).
The native PowerPC scene is no longer overwritten by the bevel fallback.
Both Systemless and SheepShaver show the blue geometry against the dark clear
color; the test requires overlapping geometry silhouettes, allowing small
rasterization differences at the edges. The entire face interior must match
SheepShaver's lighting gradient within eight levels per RGB channel, allowing
one 5-bit display quantization step. This exercises geometric normals and the
default specular material when the application omits those attributes. The 68K
checkpoint retains its labelled bevel and gauge fallback.

The Sound Manager replay is [`oracle/audio.json`](oracle/audio.json), with
capture identities and PCM extraction records in
[`oracle/audio-capture.json`](oracle/audio-capture.json). The resource contains
131,072 unsigned 8-bit samples at the classic `rate22khz` rate (about 5.89 seconds).
This leaves time for real mouse input to queue commands, flush the queue and
stop playback. The replay waits for the displayed busy, completion and Flush-issued states;
fixed guest-tick delays alone do not synchronize the host audio device.

Both Mac OS 8.1 emulators produce identical sampled-sound waveforms at full,
75% and 50% volume. The native device captures are 44,100 Hz signed 16-bit
big-endian stereo; both channels are equal. The published `.u8` evidence in
[`reference/native-audio`](reference/native-audio) retains the signed high byte
converted to unsigned mono. Extraction retains one silent sample before the
first non-silent frame and excludes device startup latency. Systemless emits
22,050 Hz unsigned mono, so the test compares every second native sample,
allowing one final sample of duration rounding, at most three 8-bit levels per
sample and a mean absolute error no greater than one level. It does not fit
the phase, gain or frequency of the waveform.
The mixer rounds its conversion increment to 16.16 Fixed precision, matching
the native recordings and avoiding accumulating phase drift in sustained audio.

The native evidence also distinguishes flush (current playback continues)
from quiet (audible output stops), and retains the callback/status readouts.
The SDL capture position is measured in written audio blocks and is not an
exact raster timestamp. SysBeep's alert waveform depends on the operating
system's configured alert sound; the integration test checks audible output
and channel lifetime separately from the strict resource-backed PCM comparison.
Set `SYSTEMLESS_TOOLBOX_AUDIO_OUTPUT` to a directory to retain the Systemless
PCM evidence from a test run (`m68k` or `ppc`, unsigned 8-bit mono at 22,050 Hz).

The TextEdit replay is [`oracle/textedit.json`](oracle/textedit.json), with
artifact identities and reviewed native results in
[`oracle/textedit-capture.json`](oracle/textedit-capture.json). Both classic
emulators select offsets 0–15 after the mouse drag. Reset selects the first
14 bytes; Copy preserves them in the private TextEdit scrap, Cut leaves 194
bytes, Paste restores 208 bytes, and typing `x` over the reset selection leaves
195 bytes. Reset then restores the original six-line buffer, and Center keeps
the selection while changing paragraph alignment. The runtime checks read the
actual text, selection, line count and private scrap, and require a visible
selection highlight. The sample uses explicit Macintosh carriage-return bytes
because MPW translates the C `\r` escape to a line-feed byte.

The complete window replay is [`oracle/windows.json`](oracle/windows.json),
with fixture and capture identities in [`oracle/windows-capture.json`](oracle/windows-capture.json).
Both emulators retain the pointer offset inside the grow box: dragging from
(425, 525) to (450, 550) grows the 320 × 245 auxiliary window to 345 × 270.
The final checkpoints show exposed-content activation and successive disposal
promoting the auxiliary window and then the main document. The hit-test point
is clear of both System 7 and Mac OS 8 window borders.

The inventory replay is [`oracle/lists.json`](oracle/lists.json), with fresh
capture identities in [`oracle/lists-capture.json`](oracle/lists-capture.json).
Intermediate checkpoints prove row 8 selection, its 40-byte mutated contents,
a full four-row scroll, compact geometry, a blank inactive scrollbar track,
and restored selection after activation. Systemless tests inspect the list's
logical cells, selected cells, visible range, geometry, and control fields.
The fixture explicitly hides list-owned scrollbars on page exit: automatic
drawing mode and activation are not substitutes for page visibility.

The portable event sequence is [`oracle/popup.json`](oracle/popup.json).
[`oracle/popup-capture.json`](oracle/popup-capture.json) records the fixture
hash, emulator source revision, display configuration, checked outcomes, and
capture hashes. Validate the recorded artifact identities with:

```sh
python3 tests/toolbox-showcase/oracle/verify_captures.py
```

This verifies provenance and detects stale artifacts; it does not rerun either
emulator or replace behavioral review. ROMs and system disks are not distributed.

## Reference screenshots

For the current sharp desktop output, see the [outline font gallery](outline-fonts/README.md),
including 4× captures, first-paint checks and the final Mac GPU output below.
Every Systemless 68K and PowerPC image in the comparison tables now shows the **4× desktop
presentation surface**, captured during the complete interaction sequence. Open an
image to inspect its full 3200×2400 resolution. Both CPU adapters retain outline
coverage through the shared presentation surface. The 800×600 guest baselines
remain in `reference/` for exact functional regression checks.

<img src="outline-fonts/mac-window/textedit.png" alt="Current Toolbox Showcase TextEdit through the Mac presentation shader" width="960">

These full-frame captures compare the shared fat fixture across implementations.
Both Systemless review galleries retain native outline detail; the oracle images retain their
original resolution.
Capture manifests record the archive revision used for refreshed oracle series.
The Systemless guest baselines are checked exactly by the integration test;
the classic-Mac images are human-review oracles because system fonts, desktop
patterns, and window chrome can vary between compatible OS installations.
The frames are functional comparisons rather than whole-frame pixel-identical
targets. Palette entry selection, colors before device-depth quantization, and
animation behavior are strict comparison points rather than presentation
variance. Cursor placement can differ between captures. The Systemless
preference gallery records Veteran difficulty, full audio, QD3D Bevels and
80% volume. The native overview separately proves changed controls, all three
hierarchical selections, and Reset Defaults at 75%; those intermediate
captures intentionally record different interaction states. Event timestamps,
accumulated counters and injected virtual key codes are not compared across
execution environments; the event kind, held input, modifier flags, queue
outcomes and cursor behavior are checked. The PowerPC run also submits native QuickDraw 3D
geometry into a visible pane. Its silhouette, clear color and interior lighting
gradient are checked against SheepShaver, including the case with no explicit
normal or specular attributes.
Classic operating-system presentation remains environment-dependent.

SheepShaver's oracle display is 32-bit direct color, so its animation checkpoint
is intentionally unchanged: *Inside Macintosh, Volume VI* (1991), p. 20-11
notes that color-table animation is unavailable on direct devices. Its
same-device transfer band likewise uses an RGB fallback because positional CLUT
indexes exist only on indexed devices. Systemless uses the same direct-color
behavior for its 16-bit PowerPC baseline; the small RGB differences in the
screenshots are the expected device-depth quantization of the same logical
colors. The Systemless 68K and BasiliskII captures exercise the actual 8-bit
indexed paths.

### Classic theme

Both galleries compare Classic System 7 rendering, with 59 checkpoints for each
architecture, stored in
[`reference/systemless-classic-68k`](reference/systemless-classic-68k) and
[`reference/systemless-classic-ppc`](reference/systemless-classic-ppc).
Both classic profiles run the same deterministic interaction sequence in CI.
BasiliskII and SheepShaver retain their native Mac OS presentation. PowerPC's
native desktop, window frames, controls, dialogs, menus and TextEdit overlays now
use the same theme provider as the 68K presentation. Monochrome native displays
retain classic rendering; guest geometry and hit testing remain unchanged.

### 68K desktop presentation

All 59 checkpoints below are captured with the default 4× outline surface enabled
before the first guest paint. Each capture also checks the unchanged 800×600 guest
baseline and verifies that its output contains actual outline detail, rather than
an enlargement of the guest framebuffer. These are presentation surfaces before
final window scaling; the [Mac shader capture](outline-fonts/README.md#final-mac-window-scaling)
separately checks that last step.

Regenerate the complete review series from the repository root:

```sh
SYSTEMLESS_REVIEW_GALLERY_DIR=tests/toolbox-showcase/review/systemless-classic-68k \
cargo test --profile ci-test --test toolbox_showcase test_toolbox_showcase -- --exact
```


| Checkpoint | Systemless | BasiliskII |
| --- | --- | --- |
| 1. Graphics | <img src="review/systemless-classic-68k/01-graphics.png" alt="Graphics page in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-01-graphics.png" alt="Graphics page in BasiliskII running the 68K slice" width="360"> |
| 1a. Main window shrunk | <img src="review/systemless-classic-68k/01-graphics-shrunk.png" alt="Main showcase window after shrinking, with exposed desktop repainted" width="360"> | — |
| 2. Controls and State menu | <img src="review/systemless-classic-68k/02-controls.png" alt="Interacted Controls page and State menu in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-02-controls-changed.png" alt="Interacted Controls page and State menu in BasiliskII" width="360"> |
| 3. Windows | <img src="review/systemless-classic-68k/03-windows.png" alt="Windows page with three overlapping windows in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/03-windows.png" alt="Windows page with overlapping windows in BasiliskII" width="360"> |
| 3a. Auxiliary activated | <img src="review/systemless-classic-68k/03-windows-aux-activated.png" alt="Auxiliary window activated above the inspector in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/03-windows-aux-activated.png" alt="Auxiliary window activated above the inspector in BasiliskII" width="360"> |
| 3b. Auxiliary moved | <img src="review/systemless-classic-68k/03-windows-moved.png" alt="Moved auxiliary window with the inspector still overlapping in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/03-windows-moved.png" alt="Moved auxiliary window with the inspector still overlapping in BasiliskII" width="360"> |
| 3c. Auxiliary resized | <img src="review/systemless-classic-68k/03-windows-resized.png" alt="Resized auxiliary window with a repaint-complete overlap in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/03-windows-resized.png" alt="Classic emulator window transition" width="360"> |
| 3d. Inspector hit-test | <img src="review/systemless-classic-68k/03-windows-hit-test.png" alt="Inspector activated through an exposed hit-test region in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/03-windows-hit-test.png" alt="Classic emulator window transition" width="360"> |
| 3e. Inspector disposed | <img src="review/systemless-classic-68k/03-windows-promoted.png" alt="Auxiliary window promoted after closing the inspector in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/03-windows-promoted.png" alt="Classic emulator window transition" width="360"> |
| 3f. Main promoted | <img src="review/systemless-classic-68k/03-windows-main-promoted.png" alt="Main window promoted after closing both auxiliary windows in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/03-windows-main-promoted.png" alt="Classic emulator window transition" width="360"> |
| 4. Drawing and 3D fallback | <img src="review/systemless-classic-68k/04-drawing.png" alt="QuickDraw drawing and 68K bevel fallback in Systemless" width="360"> | <img src="reference/basiliskii-68k/04-drawing.png" alt="QuickDraw drawing and 68K bevel fallback in BasiliskII" width="360"> |
| 5. Game preferences | <img src="review/systemless-classic-68k/05-preferences.png" alt="Changed game preferences in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-05-preferences-reset.png" alt="Changed game preferences in BasiliskII" width="360"> |
| 6. Nested menus | <img src="review/systemless-classic-68k/06-nested-menus.png" alt="File and nested Game Options menus in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-05-difficulty-menu.png" alt="File and nested Game Options menus in BasiliskII" width="360"> |
| 7. Modal dialog | <img src="review/systemless-classic-68k/07-modal-dialog.png" alt="Resource-backed game configuration dialog in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-06-modal.png" alt="Resource-backed game configuration dialog in BasiliskII" width="360"> |
| 8. Alert | <img src="review/systemless-classic-68k/08-alert.png" alt="System alert in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-06-alert.png" alt="System alert in BasiliskII" width="360"> |
| 9. Dialog result | <img src="review/systemless-classic-68k/09-dialogs.png" alt="Dialogs page after modal interactions in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-06-dialogs-complete.png" alt="Dialogs page after modal interactions in BasiliskII" width="360"> |
| 10. Initial buffer | <img src="review/systemless-classic-68k/10-te-initial.png" alt="TextEdit Initial buffer in Systemless" width="360"> | <img src="reference/basiliskii-68k/10-te-initial.png" alt="TextEdit Initial buffer in the native emulator" width="360"> |
| 10. Mouse selection | <img src="review/systemless-classic-68k/10-te-mouse-selected.png" alt="TextEdit Mouse selection in Systemless" width="360"> | <img src="reference/basiliskii-68k/10-te-mouse-selected.png" alt="TextEdit Mouse selection in the native emulator" width="360"> |
| 10. Reset selection | <img src="review/systemless-classic-68k/10-te-selected.png" alt="TextEdit Reset selection in Systemless" width="360"> | <img src="reference/basiliskii-68k/10-te-selected.png" alt="TextEdit Reset selection in the native emulator" width="360"> |
| 10. Copy | <img src="review/systemless-classic-68k/10-te-copied.png" alt="TextEdit Copy in Systemless" width="360"> | <img src="reference/basiliskii-68k/10-te-copied.png" alt="TextEdit Copy in the native emulator" width="360"> |
| 10. Cut | <img src="review/systemless-classic-68k/10-te-cut.png" alt="TextEdit Cut in Systemless" width="360"> | <img src="reference/basiliskii-68k/10-te-cut.png" alt="TextEdit Cut in the native emulator" width="360"> |
| 10. Paste | <img src="review/systemless-classic-68k/10-te-pasted.png" alt="TextEdit Paste in Systemless" width="360"> | <img src="reference/basiliskii-68k/10-te-pasted.png" alt="TextEdit Paste in the native emulator" width="360"> |
| 10. Type over selection | <img src="review/systemless-classic-68k/10-te-typed.png" alt="TextEdit Type over selection in Systemless" width="360"> | <img src="reference/basiliskii-68k/10-te-typed.png" alt="TextEdit Type over selection in the native emulator" width="360"> |
| 10. Reset edited buffer | <img src="review/systemless-classic-68k/10-te-reset.png" alt="TextEdit Reset edited buffer in Systemless" width="360"> | <img src="reference/basiliskii-68k/10-te-reset.png" alt="TextEdit Reset edited buffer in the native emulator" width="360"> |
| 10. TextEdit | <img src="review/systemless-classic-68k/10-textedit.png" alt="TextEdit interactive buffer in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/10-textedit.png" alt="TextEdit interactive buffer in BasiliskII" width="360"> |
| 11. Palette activation | <img src="review/systemless-classic-68k/11-palette.png" alt="Initial mixed-usage palette in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-08-palettes.png" alt="Initial mixed-usage palette in BasiliskII" width="360"> |
| 12. Palette animation | <img src="review/systemless-classic-68k/12-palette-animated.png" alt="Animated explicit CLUT entries in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-08-palettes-animated.png" alt="Animated explicit CLUT entries in BasiliskII" width="360"> |
| 13. Menu-bar hover | <img src="review/systemless-classic-68k/13-menu-hover.png" alt="Pages menu selected while dragging from File in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-08-menu-bar-drag.png" alt="Pages menu selected while dragging from File in BasiliskII" width="360"> |
| 14. Palette restoration | <img src="review/systemless-classic-68k/14-graphics-return.png" alt="Returned Graphics page with the default palette restored in Systemless after the 68K interaction sequence" width="360"> | <img src="reference/basiliskii-68k/overview-08-graphics-restored.png" alt="Returned Graphics page with the default palette restored in BasiliskII" width="360"> |
| 15. Lists & Inventory | <img src="review/systemless-classic-68k/15-lists.png" alt="Initial Lists and Inventory page in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/15-lists.png" alt="Initial Lists and Inventory page in BasiliskII" width="360"> |
| 15. Selected cell | <img src="review/systemless-classic-68k/15-lists-selected.png" alt="Selected cell in Systemless" width="360"> | <img src="reference/basiliskii-68k/15-lists-selected.png" alt="Selected cell in the classic emulator" width="360"> |
| 15. Mutated cell | <img src="review/systemless-classic-68k/15-lists-mutated.png" alt="Mutated cell in Systemless" width="360"> | <img src="reference/basiliskii-68k/15-lists-mutated.png" alt="Mutated cell in the classic emulator" width="360"> |
| 15. Four-row scroll | <img src="review/systemless-classic-68k/15-lists-scrolled.png" alt="Four-row scroll in Systemless" width="360"> | <img src="reference/basiliskii-68k/15-lists-scrolled.png" alt="Four-row scroll in the classic emulator" width="360"> |
| 15. Resized list | <img src="review/systemless-classic-68k/15-lists-resized.png" alt="Resized list in Systemless" width="360"> | <img src="reference/basiliskii-68k/15-lists-resized.png" alt="Resized list in the classic emulator" width="360"> |
| 15. Inactive list | <img src="review/systemless-classic-68k/15-lists-inactive.png" alt="Inactive list in Systemless" width="360"> | <img src="reference/basiliskii-68k/15-lists-inactive.png" alt="Inactive list in the classic emulator" width="360"> |
| 16. Interacted inventory list | <img src="review/systemless-classic-68k/16-lists-interacted.png" alt="Mutated, scrolled, resized, and reactivated inventory list in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/16-lists-interacted.png" alt="Mutated, scrolled, resized, and reactivated inventory list in BasiliskII" width="360"> |
| 17. Sound controls | <img src="review/systemless-classic-68k/17-sound-controls.png" alt="Sound controls in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/17-sound-controls.png" alt="Sound controls in BasiliskII" width="360"> |
| 18. Sound completion | <img src="review/systemless-classic-68k/18-sound-complete.png" alt="Sound completion in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/18-sound-complete.png" alt="Sound completion in BasiliskII" width="360"> |
| 19. Styled Text & Fonts | <img src="review/systemless-classic-68k/19-styled-text.png" alt="Styled TextEdit and Font Manager measurements in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-11-styled-text.png" alt="Styled TextEdit and Font Manager measurements in BasiliskII running the 68K slice" width="360"> |
| 20. Standard File page | <img src="review/systemless-classic-68k/20-standard-file-page.png" alt="Standard File page in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-12-standard-file.png" alt="Standard File page in BasiliskII running the 68K slice" width="360"> |
| 21. Standard File Open dialog | <img src="review/systemless-classic-68k/21-standard-file-open.png" alt="Filtered Standard File Open dialog in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-12-open.png" alt="Filtered Standard File Open dialog in BasiliskII" width="360"> |
| 22. Standard File complete | <img src="review/systemless-classic-68k/22-standard-file-complete.png" alt="Completed Standard File interactions in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-12-file-complete.png" alt="Completed Standard File interactions in BasiliskII" width="360"> |
| 23. Resource Browser | <img src="review/systemless-classic-68k/23-resource-browser.png" alt="Resource Browser enumeration in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-13-resources.png" alt="Resource Browser enumeration in BasiliskII" width="360"> |
| 24. Resource Browser loaded | <img src="review/systemless-classic-68k/24-resource-browser-loaded.png" alt="Loaded Resource Browser record in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-13-resources-loaded.png" alt="Loaded Resource Browser record in BasiliskII" width="360"> |
| 25. Resource Browser released | <img src="review/systemless-classic-68k/25-resource-browser-released.png" alt="Released Resource Browser record in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-13-resources-released.png" alt="Released Resource Browser record in BasiliskII" width="360"> |
| 26. Sprites, masks & scrolling | <img src="review/systemless-classic-68k/26-sprites.png" alt="Masked offscreen sprites in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-14-sprites.png" alt="Masked offscreen sprites in BasiliskII" width="360"> |
| 27. Animated sprite | <img src="review/systemless-classic-68k/27-sprites-animated.png" alt="Animated masked sprites in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-14-sprites-animated.png" alt="Animated masked sprites in BasiliskII" width="360"> |
| 28. Scrolled sprite scene | <img src="review/systemless-classic-68k/28-sprites-scrolled.png" alt="Scrolled offscreen sprite scene in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-14-sprites-scrolled.png" alt="Scrolled offscreen sprite scene in BasiliskII" width="360"> |
| 28a. Reset sprite scene | <img src="review/systemless-classic-68k/28-sprites-reset.png" alt="Reset sprite scene through the 4× Systemless presentation surface" width="360"> | <img src="reference/basiliskii-68k/overview-14-sprites-reset.png" alt="Reset sprite scene in BasiliskII" width="360"> |
| 29. Events & Cursors | <img src="review/systemless-classic-68k/29-events-cursors.png" alt="Events and Cursors page in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-15-events.png" alt="Events and Cursors page in BasiliskII" width="360"> |
| 30. Held mouse and queue probe | <img src="review/systemless-classic-68k/30-events-mouse-held.png" alt="Held mouse and event queue probe in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-15-events-held.png" alt="Held mouse and event queue probe in BasiliskII" width="360"> |
| 31. Key modifiers | <img src="review/systemless-classic-68k/31-events-key-modifiers.png" alt="Shift-modified key event on the Events and Cursors page in Systemless" width="360"> | <img src="reference/basiliskii-68k/overview-15-events-key.png" alt="Shift-modified key event in BasiliskII" width="360"> |
| 32. Hidden cursor | <img src="review/systemless-classic-68k/32-events-cursor-hidden.png" alt="Hidden watch cursor state in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-15-hidden.png" alt="Hidden watch cursor state in BasiliskII" width="360"> |
| 33. Final visible cursor | <img src="review/systemless-classic-68k/33-events-cursors-final.png" alt="Final visible arrow cursor state in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/overview-15-arrow.png" alt="Final visible arrow cursor state in BasiliskII" width="360"> |
| 34. Popup lists | <img src="review/systemless-classic-68k/34-popup-lists.png" alt="Popup and dropdown lists page in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/34-popup-lists.png" alt="Popup and dropdown lists page in BasiliskII running the 68K slice" width="360"> |
| 35. Popup menu tracking | <img src="review/systemless-classic-68k/35-popup-lists-open.png" alt="Tracked popup menu with separator and disabled rows in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/35-popup-lists-open.png" alt="Classic emulator popup checkpoint" width="360"> |
| 36. Popup scroll/reveal | <img src="review/systemless-classic-68k/36-popup-lists-scrolled.png" alt="Scrolled programmatic popup revealing item 36 in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/36-popup-lists-scrolled.png" alt="Classic emulator popup checkpoint" width="360"> |
| 36a. Deep item accepted | <img src="review/systemless-classic-68k/36-popup-lists-deep-selected.png" alt="Item 36 accepted in Systemless" width="360"> | <img src="reference/basiliskii-68k/36-popup-lists-deep-selected.png" alt="Item 36 accepted in the classic emulator" width="360"> |
| 37. Popup selections | <img src="review/systemless-classic-68k/37-popup-lists-selected.png" alt="Selected popup values and restored controls in Systemless running the 68K slice" width="360"> | <img src="reference/basiliskii-68k/37-popup-lists-selected.png" alt="Classic emulator popup checkpoint" width="360"> |

### PowerPC desktop presentation

These 59 checkpoints use the same 4× outline layer as 68K, retaining detail from
native PowerPC drawing and its 16-bit framebuffer through copies and restores.

```sh
SYSTEMLESS_PREFER_POWERPC=1 \
SYSTEMLESS_REVIEW_GALLERY_DIR=tests/toolbox-showcase/review/systemless-classic-ppc \
cargo test --profile ci-test --test toolbox_showcase test_toolbox_showcase -- --exact
```


| Checkpoint | Systemless | SheepShaver |
| --- | --- | --- |
| 1. Graphics | <img src="review/systemless-classic-ppc/01-graphics.png" alt="Graphics page in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-01-graphics.png" alt="Graphics page in SheepShaver running the PowerPC slice" width="360"> |
| 1a. Main window shrunk | <img src="review/systemless-classic-ppc/01-graphics-shrunk.png" alt="Main showcase window after shrinking, with exposed desktop repainted" width="360"> | — |
| 2. Controls and State menu | <img src="review/systemless-classic-ppc/02-controls.png" alt="Interacted Controls page and State menu in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-02-controls-changed.png" alt="Interacted Controls page and State menu in SheepShaver" width="360"> |
| 3. Windows | <img src="review/systemless-classic-ppc/03-windows.png" alt="Windows page with three overlapping windows in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/03-windows.png" alt="Windows page with overlapping windows in SheepShaver" width="360"> |
| 3a. Auxiliary activated | <img src="review/systemless-classic-ppc/03-windows-aux-activated.png" alt="Auxiliary window activated above the inspector in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/03-windows-aux-activated.png" alt="Auxiliary window activated above the inspector in SheepShaver" width="360"> |
| 3b. Auxiliary moved | <img src="review/systemless-classic-ppc/03-windows-moved.png" alt="Moved auxiliary window with the inspector still overlapping in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/03-windows-moved.png" alt="Moved auxiliary window with the inspector still overlapping in SheepShaver" width="360"> |
| 3c. Auxiliary resized | <img src="review/systemless-classic-ppc/03-windows-resized.png" alt="Resized auxiliary window with a repaint-complete overlap in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/03-windows-resized.png" alt="Classic emulator window transition" width="360"> |
| 3d. Inspector hit-test | <img src="review/systemless-classic-ppc/03-windows-hit-test.png" alt="Inspector activated through an exposed hit-test region in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/03-windows-hit-test.png" alt="Classic emulator window transition" width="360"> |
| 3e. Inspector disposed | <img src="review/systemless-classic-ppc/03-windows-promoted.png" alt="Auxiliary window promoted after closing the inspector in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/03-windows-promoted.png" alt="Classic emulator window transition" width="360"> |
| 3f. Main promoted | <img src="review/systemless-classic-ppc/03-windows-main-promoted.png" alt="Main window promoted after closing both auxiliary windows in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/03-windows-main-promoted.png" alt="Classic emulator window transition" width="360"> |
| 4. Drawing and QuickDraw 3D | <img src="review/systemless-classic-ppc/04-drawing.png" alt="Visible native QuickDraw 3D scene in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/04-drawing.png" alt="Visible native QuickDraw 3D scene in SheepShaver" width="360"> |
| 5. Game preferences | <img src="review/systemless-classic-ppc/05-preferences.png" alt="Changed game preferences in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-05-preferences-reset.png" alt="Changed game preferences in SheepShaver" width="360"> |
| 6. Nested menus | <img src="review/systemless-classic-ppc/06-nested-menus.png" alt="File and nested Game Options menus in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-05-difficulty-menu.png" alt="File and nested Game Options menus in SheepShaver" width="360"> |
| 7. Modal dialog | <img src="review/systemless-classic-ppc/07-modal-dialog.png" alt="Resource-backed game configuration dialog in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-06-modal.png" alt="Resource-backed game configuration dialog in SheepShaver" width="360"> |
| 8. Alert | <img src="review/systemless-classic-ppc/08-alert.png" alt="Live system alert in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-06-alert.png" alt="Live system alert in SheepShaver" width="360"> |
| 9. Dialog result | <img src="review/systemless-classic-ppc/09-dialogs.png" alt="Dialogs page after modal interactions in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-06-dialogs-complete.png" alt="Dialogs page after modal interactions in SheepShaver" width="360"> |
| 10. Initial buffer | <img src="review/systemless-classic-ppc/10-te-initial.png" alt="TextEdit Initial buffer in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/10-te-initial.png" alt="TextEdit Initial buffer in the native emulator" width="360"> |
| 10. Mouse selection | <img src="review/systemless-classic-ppc/10-te-mouse-selected.png" alt="TextEdit Mouse selection in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/10-te-mouse-selected.png" alt="TextEdit Mouse selection in the native emulator" width="360"> |
| 10. Reset selection | <img src="review/systemless-classic-ppc/10-te-selected.png" alt="TextEdit Reset selection in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/10-te-selected.png" alt="TextEdit Reset selection in the native emulator" width="360"> |
| 10. Copy | <img src="review/systemless-classic-ppc/10-te-copied.png" alt="TextEdit Copy in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/10-te-copied.png" alt="TextEdit Copy in the native emulator" width="360"> |
| 10. Cut | <img src="review/systemless-classic-ppc/10-te-cut.png" alt="TextEdit Cut in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/10-te-cut.png" alt="TextEdit Cut in the native emulator" width="360"> |
| 10. Paste | <img src="review/systemless-classic-ppc/10-te-pasted.png" alt="TextEdit Paste in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/10-te-pasted.png" alt="TextEdit Paste in the native emulator" width="360"> |
| 10. Type over selection | <img src="review/systemless-classic-ppc/10-te-typed.png" alt="TextEdit Type over selection in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/10-te-typed.png" alt="TextEdit Type over selection in the native emulator" width="360"> |
| 10. Reset edited buffer | <img src="review/systemless-classic-ppc/10-te-reset.png" alt="TextEdit Reset edited buffer in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/10-te-reset.png" alt="TextEdit Reset edited buffer in the native emulator" width="360"> |
| 10. TextEdit | <img src="review/systemless-classic-ppc/10-textedit.png" alt="TextEdit interactive buffer in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/10-textedit.png" alt="TextEdit interactive buffer in SheepShaver" width="360"> |
| 11. Palette activation | <img src="review/systemless-classic-ppc/11-palette.png" alt="Initial mixed-usage palette in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-08-palettes.png" alt="Initial mixed-usage palette in SheepShaver" width="360"> |
| 12. Palette animation | <img src="review/systemless-classic-ppc/12-palette-animated.png" alt="Direct-color palette after AnimateEntry in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-08-palettes-animated.png" alt="Direct-color palette after AnimateEntry in SheepShaver" width="360"> |
| 13. Menu-bar hover | <img src="review/systemless-classic-ppc/13-menu-hover.png" alt="Pages menu selected while dragging from File in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-08-menu-bar-drag.png" alt="Pages menu selected while dragging from File in SheepShaver" width="360"> |
| 14. Palette restoration | <img src="review/systemless-classic-ppc/14-graphics-return.png" alt="Returned Graphics page with the default palette restored in Systemless after the PowerPC interaction sequence" width="360"> | <img src="reference/sheepshaver-ppc/overview-08-graphics-restored.png" alt="Returned Graphics page with the default palette restored in SheepShaver" width="360"> |
| 15. Lists & Inventory | <img src="review/systemless-classic-ppc/15-lists.png" alt="Initial Lists and Inventory page in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/15-lists.png" alt="Initial Lists and Inventory page in SheepShaver" width="360"> |
| 15. Selected cell | <img src="review/systemless-classic-ppc/15-lists-selected.png" alt="Selected cell in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/15-lists-selected.png" alt="Selected cell in the classic emulator" width="360"> |
| 15. Mutated cell | <img src="review/systemless-classic-ppc/15-lists-mutated.png" alt="Mutated cell in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/15-lists-mutated.png" alt="Mutated cell in the classic emulator" width="360"> |
| 15. Four-row scroll | <img src="review/systemless-classic-ppc/15-lists-scrolled.png" alt="Four-row scroll in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/15-lists-scrolled.png" alt="Four-row scroll in the classic emulator" width="360"> |
| 15. Resized list | <img src="review/systemless-classic-ppc/15-lists-resized.png" alt="Resized list in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/15-lists-resized.png" alt="Resized list in the classic emulator" width="360"> |
| 15. Inactive list | <img src="review/systemless-classic-ppc/15-lists-inactive.png" alt="Inactive list in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/15-lists-inactive.png" alt="Inactive list in the classic emulator" width="360"> |
| 16. Interacted inventory list | <img src="review/systemless-classic-ppc/16-lists-interacted.png" alt="Mutated, scrolled, resized, and reactivated inventory list in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/16-lists-interacted.png" alt="Mutated, scrolled, resized, and reactivated inventory list in SheepShaver" width="360"> |
| 17. Sound controls | <img src="review/systemless-classic-ppc/17-sound-controls.png" alt="Sound controls in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/17-sound-controls.png" alt="Sound controls in SheepShaver" width="360"> |
| 18. Sound completion | <img src="review/systemless-classic-ppc/18-sound-complete.png" alt="Sound completion in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/18-sound-complete.png" alt="Sound completion in SheepShaver" width="360"> |
| 19. Styled Text & Fonts | <img src="review/systemless-classic-ppc/19-styled-text.png" alt="Styled TextEdit and Font Manager measurements in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-11-styled-text.png" alt="Styled TextEdit and Font Manager measurements in SheepShaver" width="360"> |
| 20. Standard File page | <img src="review/systemless-classic-ppc/20-standard-file-page.png" alt="Standard File page in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-12-standard-file.png" alt="Standard File page in SheepShaver" width="360"> |
| 21. Standard File Open dialog | <img src="review/systemless-classic-ppc/21-standard-file-open.png" alt="Filtered Standard File Open dialog in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-12-open.png" alt="Filtered Standard File Open dialog in SheepShaver" width="360"> |
| 22. Standard File complete | <img src="review/systemless-classic-ppc/22-standard-file-complete.png" alt="Completed Standard File interactions in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-12-file-complete.png" alt="Completed Standard File interactions in SheepShaver" width="360"> |
| 23. Resource Browser | <img src="review/systemless-classic-ppc/23-resource-browser.png" alt="Resource Browser enumeration in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-13-resources.png" alt="Resource Browser enumeration in SheepShaver" width="360"> |
| 24. Resource Browser loaded | <img src="review/systemless-classic-ppc/24-resource-browser-loaded.png" alt="Loaded Resource Browser record in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-13-resources-loaded.png" alt="Loaded Resource Browser record in SheepShaver" width="360"> |
| 25. Resource Browser released | <img src="review/systemless-classic-ppc/25-resource-browser-released.png" alt="Released Resource Browser record in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-13-resources-released.png" alt="Released Resource Browser record in SheepShaver" width="360"> |
| 26. Sprites, masks & scrolling | <img src="review/systemless-classic-ppc/26-sprites.png" alt="Masked offscreen sprites in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-14-sprites.png" alt="Masked offscreen sprites in SheepShaver" width="360"> |
| 27. Animated sprite | <img src="review/systemless-classic-ppc/27-sprites-animated.png" alt="Animated masked sprites in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-14-sprites-animated.png" alt="Animated masked sprites in SheepShaver" width="360"> |
| 28. Scrolled sprite scene | <img src="review/systemless-classic-ppc/28-sprites-scrolled.png" alt="Scrolled offscreen sprite scene in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-14-sprites-scrolled.png" alt="Scrolled offscreen sprite scene in SheepShaver" width="360"> |
| 29. Events & Cursors | <img src="review/systemless-classic-ppc/29-events-cursors.png" alt="Events and Cursors page in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-15-events.png" alt="Events and Cursors page in SheepShaver" width="360"> |
| 30. Held mouse and queue probe | <img src="review/systemless-classic-ppc/30-events-mouse-held.png" alt="Held mouse and event queue probe in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-15-events-held.png" alt="Held mouse and event queue probe in SheepShaver" width="360"> |
| 31. Key modifiers | <img src="review/systemless-classic-ppc/31-events-key-modifiers.png" alt="Shift-modified key event on the Events and Cursors page in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/overview-15-events-key.png" alt="Shift-modified key event in SheepShaver" width="360"> |
| 32. Hidden cursor | <img src="review/systemless-classic-ppc/32-events-cursor-hidden.png" alt="Hidden watch cursor state in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-15-hidden.png" alt="Hidden watch cursor state in SheepShaver" width="360"> |
| 33. Final visible cursor | <img src="review/systemless-classic-ppc/33-events-cursors-final.png" alt="Final visible arrow cursor state in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/overview-15-arrow.png" alt="Final visible arrow cursor state in SheepShaver" width="360"> |
| 34. Popup lists | <img src="review/systemless-classic-ppc/34-popup-lists.png" alt="Popup and dropdown lists page in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/34-popup-lists.png" alt="Popup and dropdown lists page in SheepShaver running the PowerPC slice" width="360"> |
| 35. Popup menu tracking | <img src="review/systemless-classic-ppc/35-popup-lists-open.png" alt="Tracked popup menu with separator and disabled rows in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/35-popup-lists-open.png" alt="Classic emulator popup checkpoint" width="360"> |
| 36. Popup scroll/reveal | <img src="review/systemless-classic-ppc/36-popup-lists-scrolled.png" alt="Scrolled programmatic popup revealing item 36 in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/36-popup-lists-scrolled.png" alt="Classic emulator popup checkpoint" width="360"> |
| 36a. Deep item accepted | <img src="review/systemless-classic-ppc/36-popup-lists-deep-selected.png" alt="Item 36 accepted in Systemless" width="360"> | <img src="reference/sheepshaver-ppc/36-popup-lists-deep-selected.png" alt="Item 36 accepted in the classic emulator" width="360"> |
| 37. Popup selections | <img src="review/systemless-classic-ppc/37-popup-lists-selected.png" alt="Selected popup values and restored controls in Systemless running the PowerPC slice" width="360"> | <img src="reference/sheepshaver-ppc/37-popup-lists-selected.png" alt="Classic emulator popup checkpoint" width="360"> |

The test loads the `.sit` once per CPU slice, waits on semantic menu and window
state rather than relying on fixed delays, and compares all 59 rendered
frames. The six additional Windows frames are deterministic Systemless
checkpoints; classic-Mac review also covers activation and movement, while the
remaining repaint lifecycle is asserted semantically because window chrome and
rasterization vary between host systems. To review and accept an intentional
rendering change, regenerate the Systemless sources and inspect the resulting PNG
diff before committing it:

```sh
SYSTEMLESS_UPDATE_TOOLBOX_REFERENCES=1 cargo test --locked --test toolbox_showcase
SYSTEMLESS_PREFER_POWERPC=1 SYSTEMLESS_UPDATE_TOOLBOX_REFERENCES=1 cargo test --locked --test toolbox_showcase
```

On the Popup & Dropdown Lists page, press `d` to disable both popup controls
through guest `HiliteControl`, or `e` to re-enable them. These keys preserve
the selected values and exercise disabled CDEF drawing and hit testing.

## GPUI standard-list paint qualification

The list capture path uses the same `Demo` compositor as the desktop frontend.
It erases qualified visible standard-cell paint to magenta in the source image
before composition. The `.guest.png` retains the original native frame, and
the JSON sidecar records the exact erased regions and actual backing-paint
depth. Application borders, custom drawing and declined cells remain native.
Magenta sampling at unowned boundaries is outside the owned-pixel comparison.

Build the example, commit the source, and use a fresh output directory:

```sh
cargo build --locked --example gpui-menu-demo --features gpui-demo-test
python3 tests/toolbox-showcase/capture-gpui-list-matrix.py /tmp/list-transitions
python3 tests/toolbox-showcase/verify-gpui-list-matrix.py /tmp/list-transitions/progress.json
python3 tests/toolbox-showcase/archive-gpui-list-matrix.py /tmp/list-transitions/progress.json /tmp/list-transitions-archive
```

The default matrix covers 48 cases: monochrome/colour 68k and PPC8/PPC16,
four scene scales, and scrolled/inactive/reactivated list states. Add
`--selected` to capture the 16 selected-state cases instead. Each transition
clicks a guest fixture button through session input. Guest selection must
survive; the visible first row must become 4 after scrolling, and the active
flag must match the requested activation state. The matrix verifier requires
every combination exactly once, checks file hashes and sidecar state, and
compares every owned device pixel with the native frame using the canvas's
device-edge rounding. The capture script pins source, fixture and binary
hashes; do not rebuild the example or edit pinned files during its run.

For an individual fractional-scale PPC16 transition capture, omit the indexed
depth argument to use the PPC16 architecture default:

```sh
target/debug/examples/gpui-menu-demo tests/toolbox-showcase/toolbox-showcase.sit --prefer-powerpc --capture-lists-transition /tmp/list-inactive.png --capture-list-transition inactive --capture-scale 0.75
python3 tests/toolbox-showcase/verify-gpui-list-text.py /tmp/list-inactive.png
```

These captures establish retained standard-cell paint and guest-button fixture
states. They do not establish general font replacement, GPUI pointer routing,
native host activation observation, complete lifecycle behavior, performance,
or release readiness. Review rendered and guest images before describing
visible behavior; retain the evidence's original scope when archiving it.

List mutation and resize captures use `--capture-lists-transition PATH --capture-list-transition mutated|resized`. They select guest row seven and click the existing guest button; they assert retained ListHandle/generation/owner, preserved selection, and exact changed bytes or bounds. After building and committing the capture source, `python3 tests/toolbox-showcase/capture-gpui-list-matrix.py /tmp/list-lifecycle --lifecycle` requests 32 cases across the four display/CPU modes and four scales. These new cases are not yet qualified. Row seven may move outside the resized viewport; the verifier compares only qualified visible paint, while guest selection remains retained. Disposal and handle reuse require separate checks.

Replacement prompt captures: `--capture-standard-file-replace-composed PATH --capture-scale SCALE` preserves the native `.guest.png` and emits a `.json` sidecar with modal state, filename, message rectangle, scale, and actual presented framebuffer depth. `python3 tests/toolbox-showcase/verify-gpui-file-replacement-text.py PATH` compares the entire message rectangle with device-edge scaling, requires native ink, and rejects incorrect depth. Use depths 1/8 for classic modes, PPC8 explicitly, and PPC16 without an indexed depth argument. New prompt captures remain pending; verifier synthetic rejection checks are not guest or oracle qualification.

Shared Demo composed fixture captures also emit a separate `.capture.json` file. It records actual depth, requested and rendered scale, scene origin, guest dimensions, logical viewport dimensions, composed pixel dimensions, capture case, and the current typography policy. Pair it with the original `.guest.png` and specialized state sidecar where available. The metadata does not qualify smooth appearance or performance, and exact binary-pixel verifiers must not be treated as smooth-font evidence.

For normal selected-list appearance, combine `--capture-lists-selected PATH` with `--capture-lists-retain-native-source`. This leaves the native source frame intact and labels the specialized sidecar as appearance-only evidence. It cannot establish GPUI ownership: the diagnostic list verifier requires magenta erased source. Use the original diagnostic capture alongside it when reviewing fractional-scale texture boundaries.

For smooth selected-list appearance review, run `python3 tests/toolbox-showcase/capture-gpui-list-matrix.py /tmp/gpui-smooth-selected --selected --smooth-review` after building and committing the source. This collects all four framebuffer modes at four scales with retained native source, pins source/binary/fixture hashes and records shared compositor metadata. It deliberately does not run the binary pixel oracle; images require visual review, and retained source can conceal incomplete replacement. Transition and lifecycle captures remain erased-source only.

`verify-gpui-smooth-list-provenance.py MANIFEST` validates the complete16-case retained-source selected matrix (or completed cases with `--partial` during capture), checking capture hashes, actual depths, shared compositor geometry and guest selection. It checks no glyph pixels and makes no raster parity claim. The capture runner invokes it after smooth review captures. Archived evidence and the exact visual-review subset are in `reference/gpui-demo/smooth-list-selected-shared`.

Smooth retained-source capture now also supports the default scrolled/inactive/reactivated matrix and `--lifecycle` mutation/resize matrix. Omit `--selected` for transitions. Capture cases normalize to the same guest transition path and only change source masking; metadata retains the explicit NativeSource case. The smooth provenance checker validates transition, activation, selection, visible owner cells, updated bytes and resized bounds, without a raster parity claim.

Archive a completed smooth list matrix with `python3 tests/toolbox-showcase/archive-gpui-smooth-list-matrix.py /tmp/CAPTURE/progress.json reference/OUTPUT`. The output must be fresh. The tool verifies complete provenance, compresses PNGs while checking identical decoded RGBA, preserves original and archived hashes, copies sidecars, and verifies the archive. If `visual-review.json` exists beside the input manifest, it copies only its explicitly reviewed samples after checking their original hashes. This never establishes glyph parity.

The completed48-case smooth scrolling/activation matrix is archived in `reference/gpui-demo/smooth-list-transitions-shared`; `review.json` names only the12 visually reviewed cases. `archive-verification.json` pins the checker and archiver hashes separately from the captured renderer source. Run the smooth provenance checker on its `progress.json` to validate depth/geometry/guest state and hashes, not glyph parity.

Completed32-case smooth mutation/resize evidence is in `reference/gpui-demo/smooth-list-lifecycle-shared`. It verifies native state/geometry and hashes, with eight explicitly reviewed samples. The review records a partial-row resize difference explained by the CPU-specific guest baseline recipes; full rows0..5 preserve exact pixels in the sampled colour68k comparison. This is fixture mutation/resize evidence, not disposal/identity reuse or full raster qualification.

### Styled TextEdit spacing controls

On page11 (Styled Text & Fonts), Option-C applies condensed spacing, Option-E
extended spacing, Option-B both flags and Option-N removes both flags. The guest
reads each character with TEGetStyle and applies only face changes with
TESetStyle, preserving font, size, colour, other face bits and selection. Normal
typing and the default sample remain unchanged. The ordinary
`macintosh_styled_spacing` regression exercises these keys, verifies runtime
CPU/depth and retained style intent, then clicks and types in condensed and
extended fields on mono68k, colour68k, PPC8 and PPC16. Current qualification
status and composed rendering gaps are recorded in GPUI_COVERAGE.md.

For guest-driven spacing captures, add `--spacing condensed|extended|both` and
`--smooth-review` to `capture-gpui-styled-text-matrix.py`; the default is normal.
For example, `--multiline --spacing condensed --smooth-review` requests all16
CPU/depth/scale cases. Build and commit clean source before starting the driver,
and keep its pinned source, executable and fixture unchanged until it exits.
Spacing metadata must agree across the request, sidecar and guest style runs.
These smooth captures require visual review and do not use a binary ink oracle.

The styled matrix driver also accepts `--binary PATH` for an explicitly built
capture executable. Its default remains `target/debug/examples/gpui-menu-demo`.
For the repository's optimized test profile, build with
`cargo build --locked --profile ci-test --example gpui-menu-demo --features gpui-demo-test`
and pass `--binary target/ci-test/examples/gpui-menu-demo`. The driver retains
its clean-source requirement and pins that exact executable, fixture and
renderer sources throughout the run. The profile retains debug assertions and
overflow checks; using it does not establish production performance. Omitting
`--multiline` requests the complete192-case single-line state matrix.

For complete smooth single-line matrices, run
`verify-gpui-styled-activation-pixels.py DIRECTORY --report REPORT` and
`verify-gpui-styled-state-visibility.py DIRECTORY --report REPORT`. The latter
checks nonempty selection/caret changes within native change bounds (one device
pixel allowance), plus exact hidden-caret equality on suspend. Run
`check-gpui-styled-state-visibility.py DIRECTORY --report REPORT` to verify five
updated-hash pixel corruptions are rejected even when restoration equality
passes. These checks qualify state visibility, not antialiased font appearance.

On Styled Text & Fonts, Option-O/S/H apply underline plus outline/shadow/both
through guest TEGetStyle/TESetStyle, preserving unmasked face bits, font, size,
colour, text and selection. Option-A requests all five basic effects. For
manual shared-compositor probes, use `--capture-styled-halo` with
`underlined-outline`, `underlined-shadow`, `underlined-both` or `everything`.
The default `normal` performs no halo mutation. Guest line metrics may change
with effects and are retained from the new snapshot. The styled matrix driver accepts `--halo` with the same values, together
with `--smooth-review`. It records the policy and explicit capture command;
the provenance verifier checks actual guest face bits, including preserved
spacing bits. Non-normal halo requests reject the unqualified binary oracle
before output creation. Full halo matrices remain unfinished.

On Styled Text & Fonts, Option-M sets the first four Pages items to outline, shadow, underlined outline+shadow, and bold+italic+underline+outline+shadow through guest SetItemStyle. Hidden capture option `--capture-standard-menu-styled` applies that guest control before opening the shared Demo Pages menu; `--capture-scale` controls the scene scale. The capture asserts actual CPU and retains all menu snapshot fields except the requested four styles. `styled-menu-halos` archives sixteen CPU/depth/scale probes; styled menu selection and keyboard tracking still need qualification.

Styled TextEdit scroll qualification uses Option-V to call guest TEScroll(0,-12) and Option-R for TEScroll(0,12). Add `--capture-styled-scroll` to `--capture-styled-text-edit-multiline PATH` to capture the scrolled field after a guest click at the second-line byte boundary. This verifies guest editing/scrolling and the shared renderer; it does not establish physical host input or automatic selection reveal.

For scrolled styled selection lifecycle captures, combine `--capture-styled-text-edit-multiline PATH --capture-styled-scroll` with `--capture-styled-caret-state suspended` or `resumed`. This selects bytes28..31 through guest mouse tracking before requesting frontend session activation, retains guest scroll geometry, and qualifies the resulting field against guest pixels. The activation request is automated session input, not a physical macOS observer check.

Generic real-application capture diagnostics accept `--capture-application OUTPUT`, optional `--capture-application-key CODE --capture-application-character BYTE`, `--capture-application-updates N` (default120), and `--capture-application-timeout-seconds N` (default30, maximum60). Input down/up defaults to observed updates120/180; `--capture-application-input-after N` and `--capture-application-hold-updates N` change that timing. `--capture-application-mouse-v V --capture-application-mouse-h H` supplies guest mouse coordinates, either alone or alongside a separately timed key (`--capture-application-key-after N`). Captures observe at least60 updates after release. These are observed production-worker snapshots, not frontend ticks or frame timing. Timeout exits as failure and saves `.timeout.guest.png` plus `.timeout.json`. Keep external application media, scenarios and captures outside tracked source; inspect the actual scene before claiming a workflow passed.
