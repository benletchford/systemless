---
id: star-trek-judgment-rites
kind: game
title: "Star Trek: Judgment Rites"
summary: Join Kirk's crew for a self-contained adventure from Interplay's classic Star Trek game.
developer: Interplay Productions
publisher: MacPlay / Interplay Productions
year: 1995
architectures: [68k, ppc]
default_architecture: 68k
category: Puzzle
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: 2026-09-20
    tester: Catalogue maintainer
    systemless_version: 0.42.1
    architecture: 68k
    environment: Deterministic headless gameplay run from the original BinHex demo image
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2234
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 32a2ba7f0 runtime, unchanged through 160f333c1
    architecture: 68k
    environment: >-
      The exact managed original matches its 98,550,322-byte receipt. Independent
      BinHex decoding and a read-only OS mount match all forty packaged files,
      eighty forks and supported Finder metadata against production packing.
      Normal intro skip and difficulty selection reach the bridge; two clicks
      advance distinct Scotty dialogue lines, against a matched no-click control.
      Twelve measured pixels and five exact replay images pass, with zero exhausted
      frames; same-runtime comparisons establish repeatability, not hardware parity.
      Full episode, navigation, combat, audio fidelity and browser approval remain
      unverified. The original demo disables saving.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5642
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 32a2ba7f0 runtime, unchanged through 160f333c1
    architecture: ppc
    environment: >-
      Container inspection identifies the real Star Trek JR PPC application in the
      same original FAT executable. Explicit PPC startup halts at unsupported
      StdCLib TrapAvailable before guest tick one. Import auditing also identifies
      additional unsupported symbols, which are potential later blockers. This
      slice and browser launch remain unqualified; keep both architectures in scope.
    status: broken
    evidence: https://github.com/benletchford/systemless/issues/5643
runtime:
  executable_path: Star Trek JR Demo
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 17009e954bea6de2d775a543ec39dd404288f6da5fb4ad223c94b9197dc6d6ce
    size_bytes: 98550322
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/star-trek-judgment-rites
    - https://static.classicmacdemos.com/demos/star-trek-judgment-rites/README.txt
    license: Interplay preview noncommercial distribution license
    rights_holder: Interplay Productions and Paramount Pictures
    permission: >-
      The included license permits no-cost copies for personal,
      noncommercial use and sharing with friends and acquaintances. It
      excludes commercial distribution and product bundles.
    notes: >-
      Unchanged 98,550,322-byte BinHex disk-image demo, SHA-256
      17009e954bea6de2d775a543ec39dd404288f6da5fb4ad223c94b9197dc6d6ce.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/star-trek-judgment-rites/bridge.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5642
    permission: Original preview gameplay captured for the existing noncommercial catalogue entry.
    notes: >-
      Fresh 68K bridge conversation from the complete unchanged managed preview.
      Cropped to the 640-by-400 game content, excluding the surrounding desktop,
      Classic Macintosh menu bar and emulator interface.
references:
- https://classicmacdemos.com/star-trek-judgment-rites
---

## One more mission

![Judgment Rites bridge conversation](incoming/star-trek-judgment-rites/bridge.png)

Judgment Rites combines dialogue, investigation and away-team puzzles in an
episode shaped like the original television series. This is Interplay's
complete Macintosh preview image. The 68K version reaches the bridge and advances
interactive dialogue. The package also includes a native PowerPC version whose
startup remains under investigation; browser launch stays disabled. Saving is
disabled in the original demo.
