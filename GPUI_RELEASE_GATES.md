# GPUI production release gates

Latest notification workflow evidence: the rebuilt dual-CPU guest fixture now
calls NMInstall from normal key events and constructs its response with
NewNMUPP. The actual production worker + shared GPUI polling/render/click
regression passes eight cases across mono68k, colour68k, PPC8 and default PPC16
(9.96s), covering autoremove and compiled callbacks, response-once and blocked
background menu input. Evidence and source hashes are under
`reference/gpui-demo/guest-notification-workflow`. This supersedes the earlier
PPC-installation gap for text-only requests. Mixed mark/icon/sound delivery,
reviewed notification images, physical input and native accessibility remain
open; all eight broad production gates remain open.

Last full gate audit: `d9324626` on 2026-10-10. Current progress reconciled
through opaque cursor presentation and production-worker folder persistence on
2026-10-11; all eight gates remain open. The production gate is the Systemless
release, as requested by the user. GPUI is already the default desktop frontend;
this document does not restore an opt-in flag. The complete objective remains
open. Passing fixture cases does not establish production readiness.

This is a current decision checklist. `GPUI_COVERAGE.md` retains chronological
implementation and evidence records; later entries can supersede earlier gaps.
A gate closes only after its complete scope is implemented and verified against
the final release candidate. No overall percentage is inferred from test counts.

| Objective requirement | Current evidence and limitation | Evidence required to close |
| --- | --- | --- |
| 1. Complete system UI inventory | Coverage inventory identifies menus, windows, dialogs, controls, lists, TextEdit, Standard File, cursors/notifications and custom-definition fallback. Its rows still describe incomplete slices and historical gaps. | Reconcile every row against current production source and tests. Enumerate supported standard variants and explicit tested custom fallbacks, CPU/depth coverage, rendering and interaction evidence. |
| 2. Shared live/headless presentation and transforms | `Demo` is shared by live presentation, composed captures and test-window input. Centered-scale interaction evidence exists. Physical host integration and complete app scenes are not established by captures. | Exercise final live compositor and transforms in representative application scenes, including clipping, overlap, scale changes and fullscreen; retain matched headless repros. |
| 3. Menu/window lifecycle | Styled menu selection128 and Escape cancellation128 pass across four CPU/depth modes and scales. Other tests cover identity, nested menus and window operations with their recorded scopes. They are not a complete final-state lifecycle qualification. | Cross-CPU mutation/disposal/reuse, callbacks, menu drag/keyboard/nesting/cancellation/visibility, window ordering/activation/drag/grow/zoom and focus restoration; verify custom-definition fallback. |
| 4. Dialogs, controls, lists, TextEdit, file workflows | Standard overlays and numerous fixture tests exist. Styled single-line text has broad state evidence. Wrapped PPC dialog text now uses original-font smooth ink and guest TextEdit geometry; composed marked-text cancellation restores exact pixels and rapid wrapped worker commits/rejection pass. Styled document TEScroll, clipping and guest click geometry now pass actual shared captures across all four modes, including PPC16 scale1.5. Scrolled selection suspend/resume also passes all four modes through the shared compositor. Scroll restoration and broader widget variants remain open. Save/replacement now verifies actual guest file contents; new-name Save persists through the production store into a fresh initialized session with exact fork/metadata comparison across four modes. New-name TEXT Save now also passes distinct writer/reader OS processes and actual guest readback across all four modes, preserving exact forks/metadata after reading. New-name file readback uses explicit production-store flush. Empty New Folder creation now survives separate reader OS processes across all four modes; actual production workers also preserve folders after Shutdown, command-channel disconnect and guest File > Quit. These checks do not isolate periodic versus final flush timing. Physical desktop close/input, crash durability, replacement restart and nonempty resource-fork restart remain open. | Complete supported standard variants, mixed custom content, multiline selection/editing/scrolling and complete Open/Save/New Folder/replacement/cancellation workflows, including file contents and persistence. |
| 5. Host input, clipboard, composition, accessibility | Key/modifier/pointer routes and clipboard bridge exist. The shared Demo now registers a GPUI text input handler for eligible document TextEdit and stages marked text. Platform test-window sequential commits pass on the earlier checkpoint; current real-worker rapid commits and persistent rejection feedback pass across all four CPU/depth modes. Separate Unicode staging now renders selection/caret with painted bounds/point queries; single/multiline shared captures preserve guest state and exactly restore pixels on cancellation. Recognized standard modal dialog handlers and real-worker rapid commits/rejection now pass across all four modes. Standard File Save/New Folder platform handlers and actual Save worker rapid commits/rejection now pass across all four modes. Wrapped PPC dialog handlers now use internal guest TextEdit geometry with four-scale interface tests and shared painted captures, retaining dialog event ownership. Styled document scrolling has actual guest/compositor evidence. Marked-text subrange corrections now preserve UTF-16 boundaries and pinned guest ownership; cross-CPU document/dialog/Save/New Folder checks cover staging, cancellation and committing corrected text. Active document vertical marked-stage range bounds/hit-testing and exact cancellation restoration now pass 32 shared captures across all four modes and scales0.75/1/1.5/2. Explicit replacement and overlapping marked-stage replacement are implemented for document, dialog, Save and New Folder targets with guest-worker evidence; Disjoint replacement now has document/dialog/Save/New Folder worker evidence, with modal/file GPUI request-ordering checks; composed visual coverage, broader staging scroll and physical IME remain unfinished. Opaque mono/colour guest cursors now use the shared Demo compositor with original pixels, hotspot and clipping. Four selected CPU/depth/scale captures establish exact hidden/inactive/outside restoration through simulated frontend states. Balanced macOS cursor hiding is implemented but physically unqualified; inverted pixels explicitly decline the complete plan. Visible Open/Save rows now expose guarded Click actions with actual guest selection tests across all four modes. Recognized document/dialog/Save/New Folder editors expose guarded SetValue using lossless Mac Roman conversion; actual-worker document4, dialog6 and file8 cases pass text/caret and stale rejection. Registration excludes host focus loss, marked staging and menu ownership. Native action delivery, text selection/navigation and full styled replacement fidelity remain open. Test-window semantic actions do not prove native accessibility. | Implement host text/composition through guest event/Toolbox paths with Mac Roman policy, caret geometry and modal ownership. Qualify physical input, repeat/wheel/capture/focus loss, clipboard formats and native accessibility actions/text editing. |
| 6. Lifecycle and native reference fidelity | Native recipe tests, guest sessions and archived native references exist with explicit limitations. Repeated Systemless captures are not independent Macintosh references. | Match CPU lifecycle scenarios and relevant reference evidence; preserve text/font/style, bounds, alignment/wrapping/clipping, caret/selection/scroll behavior and callback ownership. Record unavailable independent comparisons explicitly rather than claiming they pass. |
| 7. Representative real games | `tests/presentation_performance.rs` is ignored and requires an external archive. It measures guest execute/compose/presentation helpers, not live GPUI frame delivery. A public catalogue Marathon archive is now available under ignored target output with exact catalogue hash/size verification; a later shared Demo main-menu checkpoint now passes at800 observed production-worker updates, with an earlier30-second timeout retained. Actual guest Begin New Game input also reaches a shared Arrival level-transition checkpoint. A separately timed guest Space input now reaches an active-gameplay shared Demo snapshot. Sustained gameplay and live qualification remain open. | Use verified catalogue media and run the actual live frontend: fullscreen/hidden menu geometry, input latency, frame delivery, CPU/memory, audio continuity, saves/restart and lifecycle. Compare an established baseline under the same conditions; declare acceptable regression limits before measuring. |
| 8. Reuse, reproducibility and builds | Public fixture rebuild and prior build/package checkpoints are recorded; they pin earlier candidate states. At `43ee32a1`, post-replacement cached checks pass for the default application (10.73s) and no-default-features public library (9.21s), archived in `reference/gpui-demo/post-replacement-builds`. Earlier at `79166359`, the ordinary default-feature application passes `cargo check --locked --bin systemless` (51.22s including Cargo lock wait), and the public library passes `cargo check --locked --no-default-features --lib` (42.16s). Logs and source/hash provenance are archived in `reference/gpui-demo/current-build-checks`. A 444-file package created at 3d06c8da passes both standalone extracted library and default application checks offline, with exact frontend source hashes recorded. At production source e6690b21, the optimized default executable now links successfully (13m46s), its help path succeeds, and the existing public Graphics-to-Controls guest workflow passes on actual 68k/PPC with two assertions each and no exhausted frames. Both Controls images were reviewed. This is guest-runtime smoke coverage, not physical GPUI qualification; evidence and hashes are under `reference/gpui-demo/release-executable-smoke`. At d44b335f a new offline 446-file package has byte-identical Rust sources and passes extracted no-default public library (39.97s) and default GPUI application (48.71s) compile checks using cached dependencies/artifacts; evidence is under `reference/gpui-demo/current-packaged-source`. Network creation was interrupted after repeated DNS failures. Packaged linking/runtime and fresh-download qualification remain unproven. Fresh-download builds, final packaged tests/runtime and live qualification remain open. | Build/test the final candidate from checkout and packaged crate with supported ordinary/headless configurations. Verify published artifact contents, reproducible captures and reusable presentation modules; distinguish platform buildability from GUI qualification. |

## Current accessibility and clipboard qualification (2026-10-11)

At production source `7ec422c2`, recognized document, dialog and Standard File
editors additionally expose guarded guest text selection. Multiline document
and wrapped dialog text uses guest line boundaries, Mac Roman offsets and
clipped guest glyph geometry; inactive accessibility avoids constructing that
geometry. Actual-worker selection, stale-owner rejection and geometry/model
checks are recorded in the corresponding `reference/gpui-demo` evidence folders.
These results do not establish native accessibility tree or action delivery.
Physical activation, scrolling geometry and styled replacement remain open.

The native named-pasteboard test passes with macOS service access, preserving
all six tested hidden-format payloads after rejected export. The unchanged test
failed under restricted access when macOS returned a NULL named pasteboard.
Evidence is in `reference/gpui-demo/native-pasteboard-service-access`; it does
not qualify live clipboard suspend/resume. The full 183-test frontend suite is
not yet established as passing. All eight release gates remain open.

## Typography requirements within the gates

Recognized system text must use guest text and font/style intent, guest layout
and guest event ownership. Original-resource outlines may be antialiased at
display resolution; a modern host font must not silently replace the source or
change advances. Bitmap-only sources and unsupported paint must retain guest
rendering unless faithful replacement is established. Application-drawn text
stays guest-rendered until ownership and faithful replacement are proven.

Classic integer-scaled strikes now have original-outline support with passing
helper checks; actual guest large-font scene and composed interaction evidence
remain open. Plain PPC non-unit ratios now have passing original-outline helper
checks; supported styled non-unit recipes now synthesize source effects before
scaling with passing helper checks. Actual guest scene, composed interaction
and performance evidence remain open. These
gaps do not justify changing font metrics; unsupported paint retains guest
rendering. Broader font resources, scaled appearance and independent native
font fidelity remain open.

## Current source font-path audit (2026-10-11)

Inspected `src/bin/gpui_demo_text.rs` at production head `31ad029a`.
This is source-path evidence, not a composed appearance or Macintosh oracle.

| Recognized surface | Current path | Remaining qualification |
| --- | --- | --- |
| Dialog static text and Standard File prompts/directory labels | `classic_wrapped_text` lays out guest advances and calls `paint_smooth_label`, then retains strike ink on failure. | Supported font/face variants, wrap boundaries and CPU-specific scene fidelity. |
| Dialog single-line editor and file rows/Save filename | Original-source `ClassicLine` and `paint_smooth_label`; guest caret/selection geometry remains separate. | Original outline availability per resource and actual composed fallback coverage. |
| Styled TextEdit | Retained paint plan feeds `classic_text_pixels_with_smooth`; source styles and integer/rational scaling resolve before compositing. | Large-font and non-unit-ratio guest scenes, full styled editing and performance. |
| Labels with an explicit italic strike | `ClassicLine::styled` retains the actual italic strike instead of synthesizing a different outline. | Establish a matching original italic outline before changing this fallback. |

The inspected paths already attempt smoothing; adding another host-font label
would not establish fidelity. `resolve_smooth_run` can decline unsupported
sources/recipes, after which bitmap ink remains. Next font work should capture
a concrete failing font/size/style scene and identify its source resolution,
rather than treating every pixelated glyph as a missing GPUI call. Original
Macintosh advances and original resource identity remain mandatory.

## Next work in release-risk order

1. Implement faithful cursor inversion over the final GPUI scene and qualify
   host hiding, capture, pointer transforms and guest warps. Qualify native host
   Standard File integration and physical modal input,
   dialog/file rendering/geometry, qualify disjoint replacement ranges visually and qualify scrolling staging and qualify physical IME/focus
   boundaries. Audit native accessibility alongside it.
2. Reconcile the system UI inventory and close remaining real workflow/lifecycle
   defects, especially multiline TextEdit and durable Standard File behavior.
3. Qualify representative live applications and measure performance/audio/save
   behavior using the verified catalogue Marathon archive and additional
   representative titles as needed.
4. Extend faithful scaled text and close broader font/widget fidelity gaps.
5. Run current packaged build and executable checks beyond the completed
   release link and guest-runtime smoke checkpoint. Repeat final-candidate release audits after
   implementation is stable. Keep the PR draft and goal active until every gate is supported.

Do not expand already passing micro-style matrices without a concrete uncovered
behavior or regression. Prefer a reproduced missing end-to-end behavior and the
implementation necessary to fix it. Narrow tests remain useful regression
checks, with their scope stated explicitly.

## Guest worker progress correction (2026-10-11)

Current worker validation could exhaust the execution deadline before queued
PPC16 dialog input ran. Guest execution now receives its bounded slice after
validation; text retry settling starts after queued events drain. Final modal
regression passes six cases (52.45s), Save/New Folder four modes (107.46s), and
the default application check passes (11.13s). Evidence is archived under
`reference/gpui-demo/modal-event-progress`. An earlier monochrome New Folder
accessibility replacement timeout is retained; performance/timing and deferred
retry shutdown qualification remain open. All eight release gates remain open.

At `e0d709d8`, the complete Save/New Folder worker regression passes a second
consecutive run across four modes (80.01s). Both failures in the older 183-test
suite pass individually on current source: owner/front-window clipping (0.08s),
and named pasteboard hidden-format preservation (1.09s, macOS service access).
The older full run terminated with 180 passed, two failed and one ignored after
9449.76s; it is not current-candidate evidence. Rechecks are archived alongside
the worker correction. The earlier file timeout cause remains unproven.

The long full-suite parent remains alive at the recorded earlier checkpoint.
Subsequent targeted example builds replaced its executable path; any tests
that launch `current_exe` children can use newer artifacts. Its eventual result
must therefore be treated as mixed-checkpoint regression evidence, not a full
final-candidate qualification. Preserve that run rather than restarting on an
observation timeout; a stable final candidate needs a separate pinned artifact.

## TextEdit completed-paint ownership (2026-10-11)

Both CPU paths now retain canonical TextEdit contents/layout alongside completed
native raster evidence. Changes to guest text, font/style, wrapping/alignment,
selection, activation or caret state cannot authorize premature GPUI replacement
merely because old screen pixels remain unchanged. Completed native redraws
establish fresh ownership. Shared ownership/raster tests pass eight cases; the
actual plain-editor geometry/hit-testing/editing regression passes all twelve
CPU/depth/alignment cases (32.40s). Styled halo editing passes all four modes
(174.32s). These checks do not close physical-input, independent-font-fidelity,
performance or final-candidate qualification. Evidence is archived under
`reference/gpui-demo/textedit-paint-recipe`; all eight gates remain open.

The same candidate additionally passes styled halo editing across four modes
(174.32s) and spacing-style editing across four modes (56.85s), with the combined
log archived in `reference/gpui-demo/textedit-paint-recipe`.

### Movable dialog drag correction (2026-10-11)

Real guest title dragging now preserves dialog identity/text/selection and
translates window/item bounds in four reviewed shared compositor captures:
mono68k/0.75, colour68k/1, PPC8/2 and PPC16/1.5. PPC moving previously left
native ink behind; visible content and drawing detail now transfer from a
retained snapshot, preserving overlap and front-window occlusion. A second
defect let background FrameRect drawing cross the dialog; both port clipping
regions now apply. Final QuickDraw118 and the all-depth region regression pass;
WindowManager104 passed before the FrameRect change. Failed, partial and final
images/provenance are under `reference/gpui-demo/movable-dialog-drag`. Physical
input, broader lifecycle, performance and final-candidate qualification remain
open; no production gate is closed by this selected workflow.

### Replacement Save independent-process readback (2026-10-11)

Create/replace/read now passes twelve distinct OS-process phases across mono68k,
colour68k, PPC8 and PPC16. Persisted old data and a nonempty binary resource fork
are verified before guest replacement; guest Save requires confirmation and
updates data while preserving the resource fork. Fresh-process guest Open/read
verifies exact bytes, both forks and metadata before/after close. Stronger final
run passes20.99s; evidence is under `reference/gpui-demo/replacement-process-restart`.
This covers explicit production-store flush; actual worker shutdown/periodic
flush for replacement, crash durability and physical desktop close remain open.
All eight production gates remain open.

### Save-store crash audit (2026-10-11)

`DesktopSaveStore::persist_save_file` currently writes data fork, resource fork
and metadata directly and sequentially into the existing directory. An
interruption between writes can leave a mixed-generation save; successful
orderly exit/restart tests do not establish crash durability. Atomic snapshot
publication and interruption/recovery verification remain required before
claiming this durability scope complete. This is a source audit, not a
reproduced crash test or a claim of observed user data loss.

### Worker replacement Save at orderly exit (2026-10-11)

The actual production worker now passes replacement Save followed by Shutdown,
channel disconnect and guest File > Quit in all four CPU/depth modes (12 cases,
82.86s). No test flush is called on the replacement worker. Persisted new data
and unchanged nonempty resource fork survive a separate reader process and
actual guest Open/read/close with exact fork/metadata comparison. Evidence is
under `reference/gpui-demo/worker-replacement-shutdown`. This does not isolate
periodic/final flush timing or establish physical close/crash durability; the
sequential-write audit remains open. All eight release gates remain open.

### Atomic save snapshot checkpoint (2026-10-11)

Both forks and metadata are now staged/synced as a single binary snapshot,
published by rename and containing-directory sync. Legacy saves remain readable.
Store11 tests pass migration, interrupted publication and malformed lengths;
abrupt subprocess exits before/after publication recover complete old/new saves.
Final actual-worker replacement exits pass12 cases across four modes (88.42s),
with fresh-process guest readback. Evidence: `reference/gpui-demo/atomic-save-snapshots`.
This addresses the audited mixed-generation process-interruption risk. Sudden
power loss, tree creation/deletion durability, disk failure injection and full
storage barriers remain unqualified. No overall production gate closes here.

### Completed mixed-checkpoint frontend regression (2026-10-11)

The preserved full-suite run has now terminated successfully: 189 passed,
zero failed and one ignored in 8401.92s. Its parent began at `e0d709d8`, while
later targeted builds replaced the executable used by `current_exe` children.
This is mixed-checkpoint regression evidence, not final-candidate qualification.
A pinned packaged candidate still requires a clean, reproducible full run.

### Notification acknowledgment focus (2026-10-11)

Notification OK restores the guest root focus before dispatching the exact
owned acknowledgment, for both pointer and accessibility Click handlers.
The shared GPUI test deliberately displaces focus before clicking OK, verifies
restoration, and checks that pointer and keyboard inputs do not leak into the
guest while the alert remains owned (one test passed, 0.11s). This does not
qualify native accessibility delivery, physical focus or automatic notification
acquisition, which remains disconnected from frontend polling.

### Automatic notification acquisition (2026-10-11)

Frontend polling now requests eligible text-only alerts automatically and
retries after ownership guards clear. Only an exact worker-owned snapshot
authorizes painting/input exclusion; pending requests do not fabricate guest
ownership. The acquisition regression now advances the actual frontend timer
and verifies held-key release precedes the request. All five targeted frontend
notification tests pass (9.12s), including classic guest NMInstall/callback
worker delivery and editor text-service restoration across four modes.
These separate tests do not yet establish one composed automatic
install/display/acknowledge workflow, PPC guest installation, mixed mark/icon/
sound delivery, native accessibility or physical interaction. All release gates
remain open.

### Integrated classic notification workflow (2026-10-11)

The real-worker notification regression now feeds its installed guest snapshot
through the actual Demo polling task, waits for worker-owned acquisition, renders
the shared GPUI alert and clicks its OK button. All four classic cases pass:
mono/colour with autoremove or a Pascal leaf response (one test, 4.30s).
It verifies exact acknowledgment, response-once behavior and discarded background
menu input. This supersedes separate-path evidence for those cases; it is a GPUI
test-window workflow, not a reviewed image or physical desktop qualification.
PPC installation and mixed mark/icon/sound delivery remain unfinished.
