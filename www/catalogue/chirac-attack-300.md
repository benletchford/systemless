---
id: chirac-attack-300
kind: game
title: Chirac Attack 3.00
summary: Search a green brick maze for hidden bombs while evading a pursuing threat.
developer: Michael Doyle
publisher: Michael Doyle
year: 1995
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
    systemless_version: 0.73.0 + release browser build
    architecture: 68k
    environment: >-
      The unchanged Info-Mac package opened its 68K application. Answering Y to the
      colour prompt and choosing New Game reached a live maze. At the same guest tick,
      holding the documented D key moved the green player marker right from the start
      while the idle replay left it in place. The release browser fetched the same
      archive once, answered the colour prompt, entered the maze, and visibly moved the
      marker right with D.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3839
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 864f0c1608fd187f74c51ccb24db078cb8d87e5e6ac3c8090b406d12643f1a8e
    size_bytes: 45566
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/chirak-attack.hqx
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
    rights_holder: Michael Doyle
    permission: >-
      Michael Doyle's bundled Read Me permits sharing this game if all files,
      including the Read Me, accompany it. The author-submitted Info-Mac abstract repeats that
      grant. This entry preserves the complete archive.
    notes: >-
      Original 45,566-byte BinHex/StuffIt package with the application, maze layouts,
      and Read Me; SHA-256
      864f0c1608fd187f74c51ccb24db078cb8d87e5e6ac3c8090b406d12643f1a8e.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: b2b2c3e2ff025f4ad3071de9a65bb4878067618bf92f3c6335559e1017ee1bb8
    size_bytes: 5586
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3839
    permission: >-
      Fresh Systemless gameplay capture from the unchanged shareable package.
      Underlying artwork remains its owner's property.
    notes: Content-only active-maze crop; emulator framing is excluded.
references:
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/chirak-attack.hqx
---

## Find the bombs

![Chirac Attack maze](https://assets.systemless.org/catalogue/media/sha256/b2/b2b2c3e2ff025f4ad3071de9a65bb4878067618bf92f3c6335559e1017ee1bb8.png)

Answer **Y** to the colour monitor prompt, then choose **New Game**. Move
with **W/A/S/D**, **I/J/K/L**, or the documented number keys. Search the
maze for the hidden bomb while avoiding the pursuer. Press **P** to pause
or **Q** to end the game.
