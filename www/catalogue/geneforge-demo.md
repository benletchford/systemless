---
id: geneforge-demo
kind: game
title: Geneforge Demo
summary: >-
  Shape creatures and explore Sucia Island in Spiderweb Software's original
  Macintosh fantasy role-playing demo.
developer: Jeff Vogel
publisher: Spiderweb Software
year: 2001
architectures:
- ppc
default_architecture: ppc
category: Role-Playing
compatibility:
  status: playable
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: 5b5df152 native-equivalent runtime
    architecture: ppc
    environment: >-
      Unchanged original v1.1 MacBinary demo, default PowerPC CPU, initial display,
      partition, sound and preferences. Accept the original recommended one-time
      resolution choice, dismiss the splash with Space, choose Start New Game, accept the
      default Shaper and advance the three illustrated introduction pages. Original
      Crumbling Docks artwork, character and interface render intact. Select the character,
      click ground at (420,190) for two ticks versus omission, wait 180 ticks, press the
      original numbered reply/select key in both cases and wait 120 ticks. The active
      case triggers the shore-arrival narrative and shifts the red sprite's upper-left
      anchor 126 pixels right and 56 up; control retains its original anchor. Pre-input
      frames are byte-identical. Fresh active/control replays each pass eight measured
      baseline/narrative/location pixel checks at 2464 frontend/guest ticks, zero
      exhausted frames. Final roster click recentres active and opens inventory; control
      ends deselected with inventory hidden. Actual before/after and final captures
      inspected. This verifies bounded walking and the tutorial event, not combat, shaping,
      all seventeen areas, inventory, persistence, browser pacing or independent
      hardware/audio fidelity. Native timing is not browser performance evidence. Frozen
      native SHA-256 3c453f74dc6e2e0a7e1664fb43a0527718e64fc775a8b88de57b607744e7ebc6;
      runtime source 29d60400 has the exact tested tree merged as 5b5df152 in fix 5972. No
      emulator settings overrides, original game-file edits, cheats, unlocks or host
      extension installation.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5973
runtime:
  executable_path: Install Geneforge v1.1 Demo.app/Geneforge v1.1 ƒ/Geneforge v1.1.app
artifacts:
- id: archive
  role: archive
  format: bin
  source:
    type: sha256
    sha256: 9c12af5ed6ff2dd3186e65f022089c42990dcd1f7b61619f1621fd0a91afe988
    size_bytes: 13175040
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.spiderwebsoftware.com/geneforgeoriginal/index.html
    - https://www.spiderwebsoftware.com/geneforgeoriginal/techsupp.html
    - >-
      https://web.archive.org/web/20250825045141id_/http://www.ironycentral.com/ftp/mac/Geneforge.demo.bin
    - https://github.com/benletchford/systemless/issues/5973
    license: Spiderweb Software unmodified nonprofit distribution licence
    rights_holder: Spiderweb Software
    permission: >-
      The complete bundled Geneforge License explicitly permits nonprofit
      distribution without prior written notice provided the software is unmodified and the
      complete works are included. This entry retains the exact complete publisher download
      and all notices. The thirty-day trial, registration, noncommercial distribution
      and other original conditions remain intact. Commercial redistribution permission
      is not asserted.
    notes: "Untouched 13175040-byte MacBinary II from the publisher's explicitly linked older v1.1 download. The HTTPS Wayback snapshot is byte-identical to the current publisher HTTP original, SHA-256 9c12af5ed6ff2dd3186e65f022089c42990dcd1f7b61619f1621fd0a91afe988. No TLS verification disabled. Later installer wrapping chronology is disclosed; no period wrapper-byte identity claim. Independent unar matches both outer installer forks and Finder metadata after accounting for its AppleDouble wrapper. Separate Python archive parsing/decompression recovers all 339 VISE records: 320 file records and nineteen directories. Production KPK is 24916416 bytes, SHA-256 d5b48d2af44f05b6e8642c5eb8db9ba2177149d633815730df6cb02da97d4124; all 640 forks and 320 type/creator records match by multiset, including both conflicting .DS_Store records. Literal Macintosh slash characters use the established U+F02F VFS encoding. This is not an original installer/hardware oracle. Complete original license (7737 bytes), data-folder Read Me (839), document-viewing note (996) and support note (6283) reviewed. The 27-page PDF was extracted for notice/control review; its introduction limits the demo to seventeen of seventy-seven sections. Thirty-three selected UI/text resources were scanned and all relevant notice contexts reviewed. Actual game is PowerPC PEF with no CODE resources. All original game content, notices and bundled Apple libraries remain intact; no guest instruction tracing, disassembly, script decoding, cheats, unlocks, preference rewrites or host extension installation."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: dae2a8754e60cb0fdedaa8a02f05f8141d92d7cb4933892f6f308f73eedb0f2a
    size_bytes: 826094
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5973
    rights_holder: Spiderweb Software
    permission: >-
      Fresh Systemless capture of the original nonprofit-distributable demo;
      underlying artwork retains its original copyright.
    notes: >-
      Original unscaled 800x600 full game framebuffer after the verified ground
      click, shore-arrival narrative and normal roster selection. No desktop, emulator
      framing, cropping, colour changes or artwork edits. SHA-256
      dae2a8754e60cb0fdedaa8a02f05f8141d92d7cb4933892f6f308f73eedb0f2a, 826094 bytes. Native evidence only.
references:
- https://www.spiderwebsoftware.com/geneforgeoriginal/index.html
- https://www.spiderwebsoftware.com/geneforgeoriginal/techsupp.html
- https://github.com/benletchford/systemless/issues/5973
---

![Geneforge at Crumbling Docks after walking](https://assets.systemless.org/catalogue/media/sha256/da/dae2a8754e60cb0fdedaa8a02f05f8141d92d7cb4933892f6f308f73eedb0f2a.png)

Explore with the mouse. Click your character to select it, then click the ground
to walk. Follow the original tutorial messages; numbered choices dismiss its
narrative dialogs. Use the character portraits and check button when starting a game.

This is the original unregistered Macintosh demo. It retains the first seventeen
areas, the thirty-day trial and the publisher's registration requirements.
Native PowerPC testing verifies a ground-click walk and the first tutorial event.
Browser launch and public qualification are pending.
