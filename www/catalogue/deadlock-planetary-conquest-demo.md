---
id: deadlock-planetary-conquest-demo
kind: game
title: "Deadlock: Planetary Conquest Demo"
summary: Establish a colony on Gallius IV and compete for control of an alien world.
developer: Accolade / Lion Entertainment
publisher: MacSoft
year: 1997
architectures:
- 68k
- ppc
default_architecture: ppc
category: Strategy
launch_enabled: false
compatibility:
  status: unknown
  verified:
  - date: "2026-11-12"
    tester: Catalogue maintainer
    systemless_version: 68d3602c
    architecture: ppc
    environment: >-
      Complete unchanged original SIT and default machine settings. Candidate
      runtime from PR 6112 expands the original installer through the production
      loader. Choose Default window, acknowledge the demo notice, press Space at
      the title, then Quick Start, default Tarth race and a plains landing site.
      End Turn opens Events for turn 1; Done advances to turn 2, credits 500 to 542
      and food 100 to 96. An omitted-End-Turn control remains in the colony on
      turn 1; initial captures are RGB-identical. The complete production replay
      finishes 3444 frontend and guest ticks with zero exhausted frames and zero
      scripted assertions. All eight captures are RGB-identical to the separately
      extracted discovery run. Actual captures and external RGB comparison verify
      this bounded interaction. The 68K slice, extended play, combat, audio,
      multiplayer and browser behavior remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6113
runtime:
  executable_path: Deadlock Demo Installer/Deadlock Demo 1.0
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Deadlock%20Demo%20Installer.sit
    expected_sha256: 25738bf30fb512d41775bcf5bc4f90ae2b681b04bc23144332941b7271f2a477
    expected_size: 13982503
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/deadlock-planetary-conquest
    - https://archive.org/details/macaddict-cd-022
    - https://web.archive.org/web/20000303180735/http://www.wizworks.com/macsoft/demo_02.html
    - https://github.com/benletchford/systemless/issues/6113
    license: Maintainer-accepted promotional demo
    rights_holder: Accolade, MacSoft, Lion Entertainment and original contributors
    permission: >-
      Accepted under the maintainer-approved project policy for complete,
      unchanged, period-distributed promotional demos. This records project-policy
      acceptance without claiming an affirmative publisher licence grant. No
      applicable explicit redistribution restriction was identified in the reviewed
      notices. Preserve the complete installer, all components and variants,
      credits, notices and original demo limits, including the 40-turn solo limit,
      disabled saving/loading and restricted worlds and difficulty settings.
    notes: >-
      MacAddict 22, June 1998, includes this installer under Sponsor Demos/MacSoft.
      Its data fork is byte-identical. All 216 installer resource payloads, names,
      IDs and attributes match; raw resource-container and SIT-wrapper identity
      are not claimed. The preserved official MacSoft page advertises the demo.
      Complete production packing preserves 81 file instances, 162 forks and
      81 Finder records, matching independent unar extraction. Both same-name
      Apple Modem Tool Prefs variants remain under separate parents and were
      compared independently. The complete 21806-byte original Read Me and readable
      notice matches in 298 selected UI/text/version resources were reviewed.
      Thirty-two ADCR-compressed installer UI records remain unread and disclosed.
      The original splash and Easy Install screens were inspected through normal
      Continue; no licensing dialog was observed in that limited sequence. No
      exhaustive installer-path or artwork/text extraction is claimed. Opaque
      game resources, scripts and guest instructions were not decoded.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/deadlock-planetary-conquest-demo/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6113
    rights_holder: Accolade, MacSoft, Lion Entertainment and original contributors
    permission: Fresh content-only Systemless capture of the project-policy-accepted demo.
    notes: >-
      Unscaled 796x560 crop of actual production-loader gameplay from the unchanged
      SIT, after completing turn 1. Shows turn 2, 542 credits and 96 food. Excludes
      desktop, Classic Mac menu bar, window title and outer framing. Original
      artwork retains its copyright; no generated or retouched game pixels.
references:
- https://classicmacdemos.com/deadlock-planetary-conquest
- https://archive.org/details/macaddict-cd-022
- https://github.com/benletchford/systemless/issues/6113
- https://github.com/benletchford/systemless/pull/6112
---

![Colony on Gallius IV after advancing to turn 2](incoming/deadlock-planetary-conquest-demo/gameplay.png)

Choose Default for the window size, acknowledge the demo notice and press Space
at the title. Quick Start asks for a race and landing territory. Use the arrow at
the right of the toolbar to end a turn, then Done to close the event report.

The original solo demo ends after 40 turns and limits world sizes, difficulty
and other features. Saving and loading are disabled. Native testing verifies one
turn; browser play, extended campaigns and multiplayer remain unverified, so
browser launch stays disabled.
