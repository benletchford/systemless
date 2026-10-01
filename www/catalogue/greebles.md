---
id: greebles
kind: game
title: Greebles 1.0
summary: Drive a bulldozer through a maze to clear Greebles and their generators.
developer: Peter N Lewis
publisher: Stairways Shareware
year: 1997
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.72.0 + deterministic runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged Info-Mac Greebles 1.0 shareware archive launched in 68K
      Systemless. The Play button opened the first-run player setup; saving
      its default single-player controls and choosing Play reached Level 1:
      Pyramids. In matched deterministic runs, holding the default keypad 6
      key moved Jeremy right across the first corridor while the idle run
      left him at the starting position. The release browser build also
      reached Level 1 from the unchanged archive, and a numeric-keypad 6
      browser event moved Jeremy right along the bottom corridor. A
      three-second active sample measured about 60 host frames and 44 guest
      ticks per second.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3791
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/greebles-10.hqx
    expected_sha256: 949f319bc28a98dbb0a5377b6a141e696020f6f9fa2aad6ecbeb4de0f1de1d2e
    expected_size: 1776822
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/greebles-10.hqx
    rights_holder: Peter N Lewis and Stairways Shareware
    permission: >-
      The bundled Documentation permits distribution of the complete package
      online without charge. It reserves distribution on disk or CD for
      explicit permission. This entry preserves the complete, unchanged
      shareware archive for free online access.
    notes: >-
      Original 1,776,822-byte BinHex/StuffIt archive, SHA-256
      949f319bc28a98dbb0a5377b6a141e696020f6f9fa2aad6ecbeb4de0f1de1d2e.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/greebles/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3791
    permission: >-
      Fresh Systemless gameplay capture from the unchanged shareware package.
      Underlying artwork remains its owners' property.
    notes: >-
      Exact 642-by-483 game-content crop at (79,58) from an 800-by-600
      Systemless framebuffer in Level 1 after Jeremy moved right.
references:
- https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/greebles-10.hqx
---

## Clear the maze

![Greebles Level 1 after moving right](incoming/greebles/gameplay.png)

Choose **Play**. On first launch, select a player and save the setup, then
choose **Play** again. Jeremy's default numeric-keypad controls are **8** up,
**4** left, **6** right, **2** down, and **0** to push. The setup screen lets
you change those keys. Clear the Greebles and generators to advance.

Greebles is shareware. The bundled documentation asks players who enjoy it
to register; registration removes the periodic reminder screens.
