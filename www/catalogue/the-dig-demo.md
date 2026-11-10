---
id: the-dig-demo
kind: game
title: The Dig Demo
summary: >-
  Explore an alien world as commander Boston Low in LucasArts’ original Macintosh
  puzzle-adventure demo.
developer: LucasArts
publisher: LucasArts
year: 1995
architectures:
- 68k
- ppc
default_architecture: ppc
category: Puzzle
compatibility:
  status: unknown
  verified:
  - date: "2026-11-10"
    tester: Catalogue maintainer
    systemless_version: fb4b5071-equivalent native runtime
    architecture: ppc
    environment: >-
      Unchanged original demo with default CPU, display, partition, sound and
      preferences. Normal Command-period Skip Scene actions reach a populated puzzle scene;
      moving the pointer to (600,365) then clicking starts Boston walking right.
      Before/after captures inspected at 2052 frontend/guest ticks, zero exhausted frames. This
      verifies bounded scene entry and walking, not puzzle completion, inventory,
      saves, audio fidelity, independent hardware rendering or browser pacing. Native
      timing is not browser performance evidence. Earlier startup replays reached the scene;
      two later replays timed out before their intended input/control checkpoints and
      are retained as failures. Frozen native binary SHA-256
      a82d208ad3f5266c3a467e0019406add2e4a9a4997d98f67bfe411e048383cde.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5962
  - date: "2026-11-10"
    tester: Catalogue maintainer
    systemless_version: fb4b5071-equivalent native runtime
    architecture: 68k
    environment: >-
      Original default 68K slice reaches a populated scene through the normal Skip
      Scene sequence at 1920 frontend/2520 guest ticks, zero exhausted frames. Actual
      capture inspected. Different intro/camera timing means the PPC input checkpoint is
      not assumed to qualify walking on this slice.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/5962
runtime:
  executable_path: The Dig Demo/The Dig Demo
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 329df9e0debe2000c15b2b6355885dbc60ac44c6d13b2514f3f3316debc3c4df
    size_bytes: 22449089
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/the-dig
    - https://archive.org/details/IMG34JanFeb1996
    - >-
      https://discmaster.textfiles.com/browse/27011/macformat-033.iso/mac/Demos/Lucas%20Arts/The%20Dig%20Demo
    - https://github.com/benletchford/systemless/issues/5962
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: LucasArts
    permission: >-
      Accepted under the existing promotional-demo project policy explicitly
      authorized by the maintainer on 2026-11-10. Period cover-disc distribution, preservation
      of the complete unchanged package and absence of an explicit redistribution
      restriction in inspected applicable notices have been checked. This records
      project-policy acceptance, not an affirmative publisher licence grant. Original notices
      remain intact.
    notes: >-
      Original 22449089-byte SIT. Independent extraction matches all four files,
      eight forks and four Finder records to production preparation; strict KPK end
      45824642, SHA-256 3e705f45d98262b2829ace46c95ff89c3480a7bffc890f1274d3248a2aa1dd86. The
      complete four-file package also matches the original IMG January/February 1996
      HFS CD in both forks and Finder records, recovered through validated HTTP 206
      ranges. No whole-disc hash claim. MacFormat 33 independently corroborates the
      program/game-data forks and all 91 application resource payloads; its resource-map
      metadata differs, so whole-resource-fork identity is not claimed for that copy. Full
      4489-byte bundled Read Me, 43 selected UI/text/menu/dialog/version resources and
      retained notice matches reviewed; no explicit redistribution restriction found. The
      Read Me identifies version 1.0, 16 October 1995, as an interactive puzzle demo.
      CODE 0/1 and PowerPC PEF/cfrg slices are present. No settings rewrites,
      CPU/display/ partition/sound overrides, cheats, unlocks, guest instruction tracing,
      disassembly or script decoding used. All original files and notices retained.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: bba54019b0812730fa5c38d554b7a8452aec5db807a02a86adde0057b88134f8
    size_bytes: 61004
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5962
    rights_holder: LucasArts
    permission: >-
      Fresh Systemless capture from the project-policy-accepted promotional demo;
      underlying artwork retains its original copyright.
    notes: >-
      Unscaled 640x400 puzzle scene after ordinary walking input, cropped from the
      native frame at (80,100)-(720,500). No desktop, menu bar, emulator framing, colour
      changes or artwork edits. SHA-256
      bba54019b0812730fa5c38d554b7a8452aec5db807a02a86adde0057b88134f8, 61004 bytes. Native evidence only.
references:
- https://classicmacdemos.com/the-dig
- https://archive.org/details/IMG34JanFeb1996
- https://github.com/benletchford/systemless/issues/5962
---

![Boston Low walking in the original puzzle demo](https://assets.systemless.org/catalogue/media/sha256/bb/bba54019b0812730fa5c38d554b7a8452aec5db807a02a86adde0057b88134f8.png)

Control commander Boston Low with the mouse. Click a location to walk there,
click objects to interact, and use the **i** icon to open the inventory.
The original Read Me explains the controls and demo scope.

The Game menu’s **Skip Scene** command skips introductory scenes normally.
Native PowerPC testing verifies scene entry and a walk click. Browser play,
puzzle completion, inventory, persistence and audio fidelity remain unverified,
so browser launch is disabled.
