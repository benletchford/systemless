---
id: tristan-demo
kind: game
title: Tristan Demo
summary: >-
  Work the flippers in LittleWing's original fantasy-themed Macintosh pinball
  demo.
developer: LittleWing
publisher: LittleWing
year: 1992
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: playable
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: 2d6b4539 candidate resource-reopen runtime
    architecture: 68k
    environment: "Complete unchanged original 1.04/LC StuffIt demo, default CPU, initial display, partition, sound and preferences. Requires the generic Resource Manager correction in PR #5998. Space at the original table prompt opens the original demo notice; OK starts the table. In a matched pair, pre-input frames are RGB pixel-identical. Holding documented Z and / for six frontend ticks raises both flippers while omission leaves them lowered. After release and six ticks the active and control frames are RGB pixel-identical again. Difference while held is confined to (265,403)-(328,432). Both 1464 frontend/2064 guest ticks, zero exhausted frames; actual active, control and released captures inspected. Frozen binary SHA-256 68cf09215630362cd9bde7f765265efad13479f190e0650b56910267c1bbbff5 has the same runtime source as the rebased candidate; rebase changes release/package metadata only. Earlier CmdN-only probe remained at the start prompt and is excluded. No overrides, preference rewrites, patches, cheats or unlocks. Bounded native startup and flipper response only; full physics, nudge, completion, persistence, audio fidelity, browser pacing and independent hardware accuracy unverified."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5999
runtime:
  executable_path: Tristan™ Demo/Tristan™ Demo
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 278d01a636388a888b76285ebcf27d2a8dcb7e6548e486e7393993eb9704b602
    size_bytes: 657581
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/tristan
    - https://discmaster.textfiles.com/browse/23554
    - https://github.com/benletchford/systemless/issues/5999
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: LittleWing; original contributors
    permission: >-
      Accepted under the maintainer-approved project policy for complete, unchanged,
      period-distributed promotional demos. This is project-policy acceptance, not an
      affirmative publisher licence grant. No applicable explicit redistribution
      prohibition was identified in the reviewed original notices. Preserve the entire
      unchanged archive, copyright notices and original demo restrictions; remove it if a
      rights holder objects.
    notes: >-
      Original 657581-byte SIT contains the application, External Sound and screen1
      PICT. Independent unar agrees with production packing on all three files, six
      forks and three supported Finder type/creator/flags records. Strict KPK end 1227394
      bytes, SHA-256 3fb103c74b788e4fe712bafcb7f126eb1cd7ec7d7969c79df9855c01cc1795a9.
      The Macintosh Demo Games CD (1992-10), item23554 More Information/Tristan™, has
      matching 229 application and 29 sound resource payloads by type/id/size/hash. Whole
      forks differ in map metadata; period Finder metadata and outer SIT identity are
      not asserted. Received screen1 is absent from that period folder; it is retained
      unchanged and its original table screenshot was decoded and inspected. No
      separate readme. Reviewed 36 selected uncompressed UI/text/version/menu records and
      inspected original title/table and demo notice. Complete PICT text extraction is not
      claimed. Resource-format metadata, payload hashing and UI notices only; no guest
      instruction tracing, disassembly or game-script decoding. The original 60-second
      demo limit and disabled scoring remain intact.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: c8f516e437b96095debef824f3119b0d516c545af4876ad0abb1997eb9ca0faf
    size_bytes: 120802
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5999
    rights_holder: LittleWing; original contributors
    permission: >-
      Fresh content-only Systemless capture of the project-policy-accepted
      promotional demo; underlying artwork retains its original copyright.
    notes: >-
      Unscaled 466x356 crop (168,96)-(634,452) from the native held-flipper test.
      Excludes desktop, Classic Mac menu bar and window framing; no colour or artwork
      edits. 120802 bytes, SHA-256
      c8f516e437b96095debef824f3119b0d516c545af4876ad0abb1997eb9ca0faf. Native evidence only.
references:
- https://classicmacdemos.com/tristan
- https://github.com/benletchford/systemless/issues/5999
---

![Tristan's original demo with both flippers raised](https://assets.systemless.org/catalogue/media/sha256/c8/c8f516e437b96095debef824f3119b0d516c545af4876ad0abb1997eb9ca0faf.png)

Press **Space** at the start prompt and click **OK** on the original demo notice.
Use **Z** and **/** for the left and right flippers; **Space** nudges the table.
The original demo allows 60 seconds of play and disables scoring.
