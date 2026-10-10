# GPUI production release gates

Audited at `d9324626` on 2026-10-10. The production gate is the Systemless
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
| 4. Dialogs, controls, lists, TextEdit, file workflows | Standard overlays and numerous fixture tests exist. Styled single-line text has broad state evidence; broader multiline scrolling/editing, widget variants and complete file workflows remain open. | Complete supported standard variants, mixed custom content, multiline selection/editing/scrolling and complete Open/Save/New Folder/replacement/cancellation workflows, including file contents and persistence. |
| 5. Host input, clipboard, composition, accessibility | Key/modifier/pointer routes and clipboard bridge exist. The shared Demo now registers a GPUI text input handler for eligible document TextEdit and stages marked text. Platform test-window sequential commits pass on the earlier checkpoint; current real-worker rapid commits and persistent rejection feedback pass across all four CPU/depth modes. Separate Unicode staging now renders selection/caret with painted bounds/point queries; single/multiline shared captures preserve guest state and exactly restore pixels on cancellation. Recognized standard modal dialog handlers and real-worker rapid commits/rejection now pass across all four modes. Standard File Save/New Folder platform handlers and actual Save worker rapid commits/rejection now pass across all four modes. Inline layout, arbitrary ranges and physical IME remain unfinished. Test-window semantic actions do not prove native accessibility. | Implement host text/composition through guest event/Toolbox paths with Mac Roman policy, caret geometry and modal ownership. Qualify physical input, repeat/wheel/capture/focus loss, clipboard formats and native accessibility actions/text editing. |
| 6. Lifecycle and native reference fidelity | Native recipe tests, guest sessions and archived native references exist with explicit limitations. Repeated Systemless captures are not independent Macintosh references. | Match CPU lifecycle scenarios and relevant reference evidence; preserve text/font/style, bounds, alignment/wrapping/clipping, caret/selection/scroll behavior and callback ownership. Record unavailable independent comparisons explicitly rather than claiming they pass. |
| 7. Representative real games | `tests/presentation_performance.rs` is ignored and requires an external archive. It measures guest execute/compose/presentation helpers, not live GPUI frame delivery. A public catalogue Marathon archive is now available under ignored target output with exact catalogue hash/size verification; no live qualification is established. | Use verified catalogue media and run the actual live frontend: fullscreen/hidden menu geometry, input latency, frame delivery, CPU/memory, audio continuity, saves/restart and lifecycle. Compare an established baseline under the same conditions; declare acceptable regression limits before measuring. |
| 8. Reuse, reproducibility and builds | Public fixture rebuild and prior build/package checkpoints are recorded; they pin earlier candidate states. | Build/test the final candidate from checkout and packaged crate with supported ordinary/headless configurations. Verify published artifact contents, reproducible captures and reusable presentation modules; distinguish platform buildability from GUI qualification. |

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

## Next work in release-risk order

1. Qualify native host Standard File integration and physical modal input,
   dialog/file rendering/geometry, complete inline/wrapped staging and qualify physical IME/focus
   boundaries. Audit native accessibility alongside it.
2. Reconcile the system UI inventory and close remaining real workflow/lifecycle
   defects, especially multiline TextEdit and durable Standard File behavior.
3. Qualify representative live applications and measure performance/audio/save
   behavior using the verified catalogue Marathon archive and additional
   representative titles as needed.
4. Extend faithful scaled text and close broader font/widget fidelity gaps.
5. Run final-candidate build/package and release audits after implementation is
   stable. Keep the PR draft and goal active until every gate is supported.

Do not expand already passing micro-style matrices without a concrete uncovered
behavior or regression. Prefer a reproduced missing end-to-end behavior and the
implementation necessary to fix it. Narrow tests remain useful regression
checks, with their scope stated explicitly.
